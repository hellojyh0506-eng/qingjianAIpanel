# 青简输入质量优化总计划

日期：2026-10-10
目标：只优化用户当前正在输入的内容。正常打字不做后文联想续写；AI 不得生成不属于当前本地候选的词来替换排序结果。
范围：Core 候选排序、拼音纠错、Predict 请求/解析、Windows/macOS/CLI 候选结果消费；数字键/TSF 作为独立阶段。
原则：每个 Phase 单独验收；测试未通过不部署；不覆盖其它未提交工作；不在日志或模型请求中新增记录隐私上下文。

## 总目标与验收指标

1. 短拼音/简拼：减少找到正确词所需的翻页次数。
2. 长拼音多处错拼：提高整串意图还原率，同时控制误纠率。
3. AI：只对当前输入的本地候选排序；候选不足时才考虑专用纠错，不能续写后文。
4. 稳定性：异步返回不覆盖新输入，不抢走用户已移动的高亮或已翻到的页。
5. 性能：本地候选即时返回；AI 请求使用 debounce 和 latest-request-wins；网络失败不影响正常输入。
6. 兼容：旧 Compose API 保留为显式兼容能力；默认 Windows/macOS/CLI 打字路径不再使用续写型请求。
7. 数字输入：HALCON/文件保存对话框中主键盘、小键盘的连续数字顺序正确，TSF 不重复或倒序插入。

### 评估尺子

主要指标：Top-1、Top-5、第一页命中率、平均翻页数、多错误样本纠错召回率、正常拼音误纠率、从首键到上屏耗时、AI 重排成功率和 P50/P95 延迟。

数据来源：
- 先使用 docs/plan/model-eval.md 中的冻结评测数据与 qingjian-cli --replay，确保比较时测试样本与学习状态一致。
- 当前文档记录的 2026-09-12 冻结基线为词级 86.9%、整句 90.2%、英文 82.9%；详细口径见 docs/plan/model-eval.md。
- 实时 input-log.jsonl 被运行中的 Server 独占写入，第一轮未强行停止进程读取；因此当前报告不声称已取得桌面最新翻页分布。
- 不把应用光标前文等隐私文本另行写入日志。应用上下文如需发送给模型，必须遵循观察窗口和隐私开关。

## Phase 0：基线与回归集

状态：部分完成。

已完成：
- 读取现有 docs/plan/model-eval.md，确定冻结评测基线和评测方式。
- 确认 input-log.jsonl 的 commit 字段记录 scope、keys、pinyin、corrected、text、index、top、pages、ms 等分析信息。
- 确认 Core/Predict/Server 测试覆盖简拼、同音词、拼音纠错和过期请求。

剩余：
- 通过正常关闭或可读快照方式，安全取得当前日志副本；不强制停止正在使用的 Server。
- 统计键串长度 1-2、3-5、6-9、10+ 的第一页命中率与翻页分布。
- 从冻结样本选取高频短码（如 ba、wo、shi）与多错误长码，建立固定回归样例。

验收：能复现 Top-1/Top-5/第一页命中率/平均翻页次数基线；不导出隐私输入或应用周围文本。

## Phase 1：本地短拼音/简拼排序

状态：已完成第一轮有界实验并部署 Windows Server；实验收益有限，继续以冻结同源回放观察，不宣称整体首选准确率提升。

基线与评测：
- 仓库当前不存在历史冻结 data/eval 文件，因此本轮使用 2026-10-10 正常生产输入日志的共享只读快照（16,172 行、全部 JSON 可解析），没有停止 Server、没有修改生产配置。回放临时配置关闭云服务，只在 CLI 内存学习，生产用户词频文件不写入。
- 新增 qingjian-cli --replay 同次回放的词级输入字母长度分组报告，保证四个长度组在同一时间顺序和同一学习状态下统计。
- 当前分组基线：1–2 字母 n=1,539，Top-1 84.8%、Top-5 97.0%；3–5 字母 n=3,250，Top-1 83.7%、Top-5 96.3%；6–9 字母 n=3,535，Top-1 88.5%、Top-5 96.9%。此日志回放的用户最终选择文本存在历史引擎偏差，指标仅代表历史样本上的本地排序回放，不是独立人工标注准确率。

已完成的窄范围变更：
- 保留精确读音、输入覆盖长度、简拼数和末音节完整度这些结构性排序优先级。
- 原先同一输入串的选择次数是结构之后的硬排序项。对仅 1–2 字母的输入，只有一次的个人选择现在作为软分数加成；同一词在该输入下被选择至少两次，继续保留硬优先级。3 字母及以上、双拼/形码其他管线和旧 rank 调用路径不变。
- 同一日志快照 A/B：短码 Top-1 保持 84.8%，Top-5 从 97.0% 到 97.5%，落在第 6 名之后的数量从 46 降到 39；平均名次从 1.51 到 1.52，略有回退。整体词级 Top-1 84.2% 和 Top-5 96.0% 均不变，因此这是局部的 Top-5 改善，不代表整体准确率提升。
- 已通过 Core 332 项、CLI 12 项、Windows Server 全部测试目标（含输入循环 89 项、学习卡片/模式等），cargo check -p qingjian-windows-server、git diff --check 和 Release build。
- 代码提交：d6fe069 feat(ranking): soften one-off choices for very short pinyin。
- Windows Release 已部署：PID 25564，SHA-256 0FAACB0A2C8C86325D009195185B51550048B652AAE922A4A725D303F27CAD1D。最近部署前备份：E:\Qingjian\qingjian-server.exe.bak_intent_readings_20261010_201442；短码版本仍保留在 E:\Qingjian\qingjian-server.exe.bak_shortchoice_p1_20261010_200604；桌面验证副本哈希一致，TSF DLL 未替换。

不做：
- 不盲目扫全局常数，不降低拼音/音节结构约束，不把 AI 生成词直接插入候选。
- 不把此轮历史日志回放的改善解读为人工真值准确率；后续若取得稳定独立样本，应再验证误置顶率与翻页体验。

## Phase 2：受约束 AI 当前候选重排

状态：top27 扩展版已部署到 Windows 正常安装版，等待真实在线与交互验收；尚未证明准确率/翻页指标改善。

本阶段改动：
- 新增 PredictionKind::Rerank 和 Engine::request_rerank；默认 Windows/macOS/CLI 打字入口调用独立重排接口。
- 旧 request_prediction / Compose 保留兼容，不再由默认打字入口调用。
- 请求最多提供排名前 27 个本地中文/整句候选（大致前三页），让第 2/3 页的正确候选有机会进入第一页；want_sentence=false、max_items=0。
- Rerank 专用 Prompt 仅要求 candidate_order。
- 解析器要求返回本地候选完整排列，拒绝新增、缺失、重复或改写候选。
- Core 布局方法只在现有候选槽之间重排，不创建或删除候选。
- Windows/macOS 消费端将 Rerank 与云词插入路径分开；用户已翻页或移动高亮时不应用异步重排。
- 重排应用后更新当前输入候选顺序快照，保持选择日志与屏幕实际顺序一致。
- 少于两个候选时不请求模型；Windows 学习面板仍回退到被动翻译，避免 Loading 卡住。

尚待确认：
- macOS UI 代码需在 Apple target 上编译；当前 Windows 机器因 objc2 只允许 Apple target，无法用本机 cargo check 验证。
- 2026-10-10 Windows Release（top27 候选池、Rerank 输出上限 512 tokens、整串意图提示、受限双编辑拼音恢复）已部署并保持运行：SHA-256 `416E3C4F4D00542161C74BE48B9B73195DD1F1A1D46529F32E86FD7D3F9B02FB`，PID 为 `17352`；部署前 top27 版本备份为 `E:\Qingjian\qingjian-server.exe.bak_rerank_multiedit_20261010_181146`。桌面验证副本哈希与线上文件一致，TSF DLL 未替换。
- Windows 已复用 TSF 既有的 Surrounding 消息，把最多 64 个字符的光标前文用于当前候选重排；不发送光标后文。TSF 在私密输入框不读取前文，Server 在进入私密和组句结束时清空缓存并使在途重排失效。应用上下文的真实质量取决于宿主控件是否允许 TSF 读取。
- 需真实模型验证 JSON 输出、延迟与缓存；单元测试不等于在线验收。

验收：
- 只允许本地候选完整排列；不能引入新词或后文续写。
- 新按键后旧请求不能覆盖新候选。
- 用户已翻页/移动高亮后不重排。
- 对照数据证明第一页命中率改善且没有明显回归；未达标时不启用默认重排或调整触发阈值。

## Phase 3：长拼音多处错拼纠正

状态：第一版受限双编辑恢复已部署；真实用户样本覆盖率与冻结语料误纠率仍待评估。

目标：由单次编辑纠错增强为有界的多处拼音错误搜索，覆盖相邻错键、换位、漏字、多字母和错误音节边界。

计划：
1. 为真实多错拼输入建立冻结样例并标注正确文本/读音。
2. 在 crates/qingjian-core/src/correction/ 扩展有界搜索，而非无限组合编辑。
3. 联合词典/音节合法性、编辑代价、整句语言模型和个人敲错表评分。
4. 与 Viterbi 整句路径和错拼边衔接，避免全局纠错触发后关掉其它音节纠错变体。
5. 只有本地候选都不合理时才考虑独立 AI 当前输入修复；返回内容必须经过拼音与词汇校验，AI 不得静默覆盖已提交文本。

已完成第一版：仅对 7–24 字母的合格输入启动，且仅当原串整句不可用、单编辑纠错无有效结果时触发；最多两处编辑，必须完整切分并经本地词典/语言模型打分；组合分组限额后候选最多 256 条。复合编辑支持消耗长度映射、拼音框两处错误标记和逐音节错拼学习。回归例 `nihoomma` 在测试词典下恢复为 `nihaoma`（“你好吗”），合法输入 `nihaoma` 不触发多编辑误纠。通过 Core 328/328、Predict 30/30、Windows Server 13/13、cargo check、git diff --check 和 Release build，并已部署。仍待冻结语料和匿名多错拼样本的召回率/误纠率与 P50/P95 延迟对照；当前缺少冻结 replay 数据，不宣称整体准确率已提升。

## Phase 3.1：AI 整串长拼音意图纠错（Windows）

状态：2026-10-10 已部署 Windows Server；全套本地回归通过。真实模型在线准确率与延迟验收仍待进行，不宣称真实输入上的召回率已有提升。

实现与边界：
- 新增独立 PredictionKind::Intent，不与候选重排 Rerank 混用。触发条件为 7–64 位小写全拼字母、未被本地纠错处理、没有已按整串读音精确匹配的本地候选，且非英文、双拼、注音、形码、问字、表达或私密输入。
- 请求包含实际键串、本地切分、最多 9 个本地候选提示、候选的可信词典读音（由 Candidate.syllables 构建）和可用的光标前文；不发送光标后文、输入历史或 AI 对话历史。
- 模型只能返回 corrected_pinyin，不能直接指定汉字、补写后文或添加候选。请求使用 220 ms 防抖；Core 检查与原串的有界编辑距离（2/3/4/5）、完整合法拼音切分与本地词典整句转换。不能通过本地校验即丢弃，不显示、不上屏。
- 通过校验的中文建议复用拼音行右侧的提示位显示；原拼音保持不变，用户按 Tab 才采用。正常候选与数字/空格选词路径不变。
- Learn 模式保留现有 Rerank → Translate 次序，在被动翻译后才请求意图建议；避免两个功能争用同一个预测通道。新输入会使过期结果失效。
- 回归：Core 331 项、Predict 31 项；Windows Server 完整测试目标均通过，包括意图纠错端到端用例、学习卡片/模式、IPC、输入循环与兼容性；cargo check、git diff --check 和 Release build 通过。
- 当前线上 Server：PID 25564；SHA-256 0FAACB0A2C8C86325D009195185B51550048B652AAE922A4A725D303F27CAD1D。最近一次部署前备份：E:\Qingjian\qingjian-server.exe.bak_intent_readings_20261010_201442；短码版本备份于 E:\Qingjian\qingjian-server.exe.bak_shortchoice_p1_20261010_200604，意图纠错早期版本仍备份于 E:\Qingjian\qingjian-server.exe.bak_intent_reviewfix_20261010_194036，首次版本备份于 E:\Qingjian\qingjian-server.exe.bak_intent_20261010_193709。桌面验证副本与线上哈希相同。未替换 TSF DLL。
- 真实模型初次烟测（Qwen/Qwen3-14B）收到 HTTP 200、约 1.4 秒响应，但对合成输入 kaifazhes 返回空建议；该结果说明不能把请求成功当作纠错成功。已增加本地候选读音提示并更新 Prompt，仍须用受支持的真实在线通路复测建议质量、误纠率和 P50/P95 延迟。测试里的 Mock 响应只证明隔离、校验、显示与 Tab 采用闭环，不等于当前 Prompt 的真实模型效果已验证。

## Phase 4：HALCON / File Dialog 数字输入

状态：待只读诊断，尚未修改 TSF。

现象：中文输入法开启时连续输入 261009，后续数字可能插入到左侧，形成类似 0126；在 HALCON 和其它文件对话框出现，但是否所有控件都复现尚未确认。

重点文件：
- apps/windows/tsf/src/com/service/key_sink.rs：主键盘/小键盘虚拟键处理与是否消费按键。
- apps/windows/server/src/dispatch/key/input.rs：组句外数字/标点处理。
- apps/windows/tsf/src/com/edit/update.rs：EditSession 文本插入和光标更新。
- apps/windows/tsf/src/com/composition/：组合文本、marked text 和光标状态。
- apps/windows/tsf/tests/real_host_keyboard_empirical.rs：真实宿主键盘测试。

计划：
1. 分别在记事本、HALCON 保存对话框测试主键盘、小键盘、NumLock、中英文模式。
2. 只记录键码、处理决策、光标前后位置、EditSession 开始/结束时间，不记录输入内容。
3. 确定是 VK 映射、异步插入/光标恢复、旧 EditSession 延迟还是组句残留状态。
4. 优先修确切根因；不盲目强制所有数字键原生放行，避免破坏自绘输入控件。
5. 仅确有必要时修改 TSF DLL，单独构建/安装/注册并保留旧 DLL 回滚方案。

验收：主键盘和小键盘连续 100 次输入数字串顺序和光标正确；HALCON/File Dialog、记事本和普通文本框通过；退格、选中替换、NumLock、切换输入法无回归；不向 TSF 引入网络或 AI。

## Phase 5：完整回归、性能对照与部署

状态：待实施。

每阶段提交前：
1. cargo fmt --check
2. cargo test -p qingjian-core --lib
3. cargo test -p qingjian-predict --lib
4. cargo test -p qingjian-windows-server --lib
5. cargo check -p qingjian-windows-server
6. git diff --check
7. 用同一份冻结数据运行 qingjian-cli --replay 或模型评测工具。
8. Windows 部署前备份 E:\Qingjian\qingjian-server.exe，校验 Release 和桌面验证副本 SHA-256 一致；非必要不替换 TSF DLL。
9. macOS 改动在 Apple target CI/Mac 机器编译；Windows 上 objc2 的平台错误不代表 macOS 源码本身错误。
10. 每个 Phase 独立提交；临时截图、demo 和脚本不自动加入提交。若远端仍返回 HTTP 403，保留本地提交并明确说明。

## 当前风险边界

- 当前 Rerank 已部署 Release，但尚未完成真实模型的准确率、延迟及弹窗交互验收，因此不能宣称第一页命中率已经改善。
- Windows Server 只使用 TSF 允许提供的最多 64 个 UTF-16 单元光标前文，不读取光标后文；密码/私密输入框跳过读取，具体可获得的上下文取决于宿主控件。
- 本次未修改 TSF 数字键处理；数字输入问题仍处于独立诊断阶段。
- 不为读取实时输入日志而强行停止运行中的 Server。