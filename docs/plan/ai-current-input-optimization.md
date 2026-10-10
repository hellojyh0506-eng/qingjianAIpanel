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

状态：待实施；已完成结构审查，尚未更改本地排序权重。

目标：减少短拼音正确词落到第二页以后的情况，不依赖 AI 网络。

计划：
1. 用冻结日志定位高频多页输入。
2. 检查 crates/qingjian-core/src/ranking/scored.rs 的排序键：精确命中、覆盖长度、简拼数、末音节完整度、个人选择与上下文得分。
3. 对短拼音/简拼采用有界上下文加权；保留拼音匹配约束，不让纯语义相似但读音错误的词跃居首位。
4. 检查 Learner 同输入串选择和个人 n-gram 是否有效用于短输入。
5. 以 Top-1、第一页命中率、翻页次数、误纠率比较基线；默认配置应无需用户手工调整。

不做：不靠一次全局权重扫荡掩盖缺词；不降低拼音匹配约束；不直接把 AI 结果变成新候选。

## Phase 2：受约束 AI 当前候选重排

状态：第一版已部署到 Windows 正常安装版，等待真实在线与交互验收；尚未证明准确率/翻页指标改善。

本阶段改动：
- 新增 PredictionKind::Rerank 和 Engine::request_rerank；默认 Windows/macOS/CLI 打字入口调用独立重排接口。
- 旧 request_prediction / Compose 保留兼容，不再由默认打字入口调用。
- 请求最多提供第一页 9 个本地中文/整句候选；want_sentence=false、max_items=0。
- Rerank 专用 Prompt 仅要求 candidate_order。
- 解析器要求返回本地候选完整排列，拒绝新增、缺失、重复或改写候选。
- Core 布局方法只在现有候选槽之间重排，不创建或删除候选。
- Windows/macOS 消费端将 Rerank 与云词插入路径分开；用户已翻页或移动高亮时不应用异步重排。
- 重排应用后更新当前输入候选顺序快照，保持选择日志与屏幕实际顺序一致。
- 少于两个候选时不请求模型；Windows 学习面板仍回退到被动翻译，避免 Loading 卡住。

尚待确认：
- macOS UI 代码需在 Apple target 上编译；当前 Windows 机器因 objc2 只允许 Apple target，无法用本机 cargo check 验证。
- 2026-10-10 Windows Release 部署 SHA-256：F89ADF3A1204F0C2F1053731D42E99D01244341A5253BD10EAC4711F1AECFF7D；旧版备份为 E:\Qingjian\qingjian-server.exe.bak_rerank_20261010_171422。
- Windows 当前的 rerank 请求传入 None 上下文，只有拼音和本地候选；语义判断受限。后续要单独研究 TSF 能否安全提供光标周围文本。
- 需真实模型验证 JSON 输出、延迟与缓存；单元测试不等于在线验收。

验收：
- 只允许本地候选完整排列；不能引入新词或后文续写。
- 新按键后旧请求不能覆盖新候选。
- 用户已翻页/移动高亮后不重排。
- 对照数据证明第一页命中率改善且没有明显回归；未达标时不启用默认重排或调整触发阈值。

## Phase 3：长拼音多处错拼纠正

状态：待实施。

目标：由单次编辑纠错增强为有界的多处拼音错误搜索，覆盖相邻错键、换位、漏字、多字母和错误音节边界。

计划：
1. 为真实多错拼输入建立冻结样例并标注正确文本/读音。
2. 在 crates/qingjian-core/src/correction/ 扩展有界搜索，而非无限组合编辑。
3. 联合词典/音节合法性、编辑代价、整句语言模型和个人敲错表评分。
4. 与 Viterbi 整句路径和错拼边衔接，避免全局纠错触发后关掉其它音节纠错变体。
5. 只有本地候选都不合理时才考虑独立 AI 当前输入修复；返回内容必须经过拼音与词汇校验，AI 不得静默覆盖已提交文本。

验收：多错拼样例召回率上升；正确拼音误纠率不高于基线；候选生成有时间上限；AI 失败不影响普通输入。

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

- 当前 Rerank 是第一步架构实现，尚未在线验收或部署 Release。
- Windows 当前未向模型提供应用光标周围文本，所以不能宣称具备完整上下文理解。
- 本次未修改 TSF 数字键处理；数字输入问题仍处于独立诊断阶段。
- 不为读取实时输入日志而强行停止运行中的 Server。