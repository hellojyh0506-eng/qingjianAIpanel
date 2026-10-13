# 青简：智能拼音识别（AI 纠错）/ 学习助手面板 / AI 问答面板 —— 重构执行计划 v2

分支：`owner/v0.1.4-english-learning`（基线 `88921ba`）
参考图：`docs/design/reference/`（strict_* = 《严格参考图》，spec_* = 《界面规范总览》）
头像：`assets/ui/avatars/`（已生成，可直接用）
执行者：Sonnet。按本文件逐个 Phase 执行，每个 Phase 一个提交，测试不过不进下一步。

## 已锁定的产品决定（2026-10-13 用户确认）

1. **不做整句补全、不做续写。**「AI 联想」的含义改为：用户在电脑上连续敲一长串拼音（可能有错拼、字母颠倒、漏字母、多字母、音节边界错），系统能**结合光标前后文**识别出用户真正想打的字，并推荐出来。
2. **把握大时直接放到第 1 位，并在候选上标「纠」**；原来的第 1 位降到第 2 位，按 2 即可选回。
3. **模型分两档**：打字用更快的小模型（`typing_model`，先测速再定，候选见 P6）；学习助手和 AI 问答面板用 `panel_model` = `Qwen/Qwen3-14B`，**关掉思考**。
4. 界面冲突时以《严格参考图》为准；规范总览只补充严格图里没有的状态和颜色规范。
5. 对话头像使用已生成的 `assets/ui/avatars/` 素材。

---

## 0. 现状诊断（为什么慢、为什么不智能）

| # | 根因 | 证据 | 影响 |
|---|---|---|---|
| R1 | Qwen3 思考模式没关。SiliconFlow 上 Qwen3-14B 默认 `enable_thinking=true`，代码只给智谱发关思考参数 | `crates/qingjian-predict/src/chat_client.rs:328` `ThinkingSwitch::for_url` 只认 `bigmodel.cn / z.ai`；`stream_client.rs:33` 请求体没有思考开关 | 每次请求先思考再回答，延迟翻倍；Intent `max_tokens=96` 被思考吃光 → 空建议 |
| R2 | 一条通道串行：Engine 只有一个 `prediction_sequence` 和 `last_prediction_kind`，Worker 单循环、新请求打断旧请求 | `engine/prediction/mod.rs:601`；`worker.rs`；`dispatch/composed/mod.rs:131-153` | Rerank 2.1 s → Translate 0.7 s → Intent 1.1 s，约 4 s 才出纠错 |
| R3 | Rerank 要模型逐字回显 27 个候选的完整排列，最多 512 token，漏一个整份作废 | `prompt.rs:173`、`prompt.rs:763` | 慢、脆 |
| R4 | Intent 只能从「错拼串自己的」前 9 个本地候选里挑；正确词不在里面就救不回来 | `engine/prediction/mod.rs:81-166`、`:254` | 真正的错拼大多救不回 |
| R5 | 本地纠错覆盖不全：整串 1 处编辑（4–64 位）、音节级敲错边（**只对已经切得出的完整音节**）、两处编辑（只在 7–24 位且整句不可用时）。长串里多处把切分打断的错误没有覆盖；快速提示只认 ≥7 位 | `correction/mod.rs:29-38`、`correction/typo/mod.rs`、`engine/correcting.rs:43,125`、`engine/prediction/mod.rs:214` | 「一长串拼音里错好几处」识别不出来 |
| R6 | Windows 只送光标**前** 64 字，光标后文写死为空 | `dispatch/mod.rs:297-305`；TSF `com/edit/surrounding.rs:50` 只有 `text_before_caret` | 无法「根据前后文」判断 |
| R7 | 纠正结果没有可见标记，用户分不清哪个是纠正过的 | `Candidate` 没有纠正标记字段（`candidate/mod.rs:30-48`） | 体验上「不智能」 |
| R8 | 冷连接、温度 0.3 | `chat_client.rs` `http_client()`、`TEMPERATURE` | 停顿后首个请求多 200–400 ms |

面板问题：

| # | 问题 | 证据 |
|---|---|---|
| U1 | 三个标签都是**流式 Markdown 文本**，用通用 Markdown 渲染画出来，和参考图的卡片结构（标签胶囊、等级、序号卡、对话气泡）在原理上就对不上；`CompanionData.alternatives / dialogue` 这些结构化字段从来没被填过 | `dispatch/mod.rs:434-500`；`ui/companion.rs:726` `parse_markdown_blocks(stream_text)` |
| U2 | GDI + Unicode 字符当图标（`⌁ — ✕ 📌 ✦`），没有矢量图标、没有头像、没有抗锯齿圆角和阴影 | `ui/companion.rs:611-614`；`ui/ai_assistant.rs` |
| U3 | 点击区域和绘制坐标**各算一遍**，按钮宽度写死 112 px，一改布局就错位 | `ui/companion.rs:960-1075` |
| U4 | 「真实对话」写死成 IELTS 6–8 轮考官对白；参考图是场景切换胶囊（美式日常 / Gen Z 口语 / IELTS 口语 / 更多场景）+ 3 轮 A/B 气泡 + 表达亮点 | `dispatch/mod.rs:452` |
| U5 | 三个标签打开时同时发 3 个流式请求，互相抢带宽；每个增量都整窗重排 + `UpdateLayeredWindow` | `dispatch/mod.rs:434`；`companion.rs:234` |
| U6 | AI 面板缺：左侧会话栏、新对话、会话历史（条数 · 日期）、设置、当前主题栏、清空会话、欢迎卡片、4 个建议胶囊、附件/截图图标、蓝色发送键；会话不落盘 | `ui/ai_assistant.rs` `AiAssistantData` |
| U7 | 文件超出仓库规定的 800 行：`companion.rs` 1100、`ai_assistant.rs` 1344、`prompt.rs` 1981 | `docs/contributing.md:27` |

---

## 1. 目标与验收指标

| 指标 | 现在 | 目标 |
|---|---|---|
| 本地候选出现 | ≤ 2 ms | 不变（任何改动不得变慢，P95 ≤ 3 ms） |
| 本地纠错候选出现 | 仅部分场景 | 4 位以上全覆盖，≤ 5 ms |
| AI 智能识别结果到达（停顿后） | ≈ 4 s，常为空 | P50 ≤ 700 ms，P95 ≤ 1500 ms，空/坏回复 < 5% |
| 错拼识别（120 条错拼样本） | 未统计 | 本地 Top-1 ≥ 55%；本地 + AI Top-1 ≥ 80%，Top-3 ≥ 90% |
| 误纠率（300 条正确拼音被改成别的第 1 位） | 未统计 | ≤ 1% |
| 上下文同音选词（60 条：他/她、的/地/得、在/再 等） | 未统计 | Top-1 ≥ 85% |
| 学习助手 | 首字等待长 | TTFT ≤ 800 ms，首卡片 ≤ 1.2 s；再次打开 ≤ 50 ms |
| 面板外观 | 不一致 | 与参考图逐区块对齐：布局 ±2 px、颜色 token、所有图标矢量 |

---

## 2. 执行顺序

```
输入线： P1 提速+测速 ─► P2 多通道 ─► P3 本地纠错增强 ─► P4 AI 智能识别 ─► P5 光标后文(TSF) ─► P6 选定打字模型
界面线： P7 面板渲染基础 ─► P8 学习助手面板 ─► P9 AI 问答面板
收尾：   P10 回归、文档、部署
```
两条线互不依赖。建议先完成 P1–P4（用户最在意的打字体验），再做界面线。P5 要换 TSF DLL，单独提交、单独部署。

---

## P1　立即提速 + 测速工具（约 1 天）

**改动**
1. `chat_client.rs`：`ThinkingSwitch` 新增 `EnableThinkingFlag`，对 `siliconflow.cn`、`siliconflow.com`、`dashscope.aliyuncs.com` 发 `"enable_thinking": false` 并移除 `reasoning_effort`。
2. 打字类请求（Translate、P4 的 Resolve、现有 Rerank / Intent）**强制关思考**，不受配置影响；只有 DeepLearn / Compare / AI 聊天「深度」模式允许思考。
3. `stream_client.rs`：请求体加 `#[serde(skip_serializing_if = "Option::is_none")] enable_thinking: Option<bool>`，同样按 host 填 `Some(false)`。
4. 温度：打字类 `0.0`；Translate `0.3`；面板 `0.5–0.7`。
5. 连接预热：Server 启动时、以及组句开始且距上次请求 > 45 s 时，后台发 `max_tokens=1` 的请求；`pool_idle_timeout` 改 300 s。
6. 配置拆模型：`PredictConfig` 新增 `typing_model: String`、`panel_model: String`（空 = 回落到 `model`）。打字通道用 `typing_model`，学习助手 / AI 问答用 `panel_model`；`dispatch/mod.rs:415-425` 与 `:882-896` 两处硬编码改为读 `panel_model`。同步 `crates/qingjian-platform/src/config/mod.rs` 的配置模板和测试。
7. 测速工具：`apps/cli` 新增 `--ai-bench <samples.tsv> --models a,b,c --rounds 3`，对每个模型逐条请求真实接口，输出：P50/P95 总耗时、TTFT、空/坏回复率、Top-1 / Top-3、误纠率。结果写 Markdown 表到 stdout。API 密钥只从环境变量 / 配置读，不打印。
8. 样本集 `apps/cli/fixtures/typing_samples.tsv`（列：`letters  before  after  expected  category`），至少：
   - 120 条错拼（category = `typo`）：相邻键（`nihap→你好`）、字母颠倒（`nihoa→你好`、`zhnog→中`、`xainzai→现在`、`gognzuo→工作`）、漏字母（`zhngguo→中国`、`kaifzhe→开发者`）、多字母（`kaifazhes→开发者`、`nihaoo→你好`）、音节边界错、一串里多处错（`wojintainquchifna→我今天去吃饭`、`nihoommazuijinmang→你好吗最近忙`）；
   - 300 条正确拼音（category = `clean`），从 `docs/plan/model-eval.md` 的冻结集抽，用于误纠率；
   - 60 条上下文同音（category = `context`）：`before="今天她吃饭但是" letters="ta" expected="她"`、`before="跑" letters="dekuai" expected="得快"`、`before="我明天" letters="zailai" expected="再来"` 等。
9. 结果记录 `docs/notes/ai-latency-baseline.md`。

**测试**：三种 host 的请求体快照（有/无 `enable_thinking`）；`typing_model` / `panel_model` 回落逻辑。
**验收**：`--ai-bench` 跑通；Intent 空回复率 < 5%；P50 较改前下降 ≥ 40%。
**提交**：`perf(predict): 关闭 Qwen3 思考并拆分打字与面板模型`、`test(cli): 新增 ai-bench 测速与样本集`

状态：已完成（2026-10-13，Linux 沙箱）。`perf(predict)` 关思考 / 拆模型 / 预热、`test(cli)` ai-bench 与 480 条样本，predict / platform / cli 测试全过。**未验证**：真实接口延迟（沙箱无密钥，见 `docs/notes/ai-latency-baseline.md`）、Windows Server 编译（无 mingw，仅改了两处取模型名）。本地基线：typo Top1 64.2%、clean 97.0%、context 40.0%。

---

## P2　多通道并行（约 1–1.5 天）

1. `qingjian_core::PredictionLane { Typing, Learning, Deep }`；`PredictionKind::lane()`：Typing = Rerank / Intent / Resolve；Learning = Translate；Deep = DeepLearn / Compare / Question。
2. Engine：`prediction_sequence: [u64; 3]`、`last_kind: [PredictionKind; 3]`；`poll_prediction` 按通道判断过期；`Prediction` 带 `lane`。
3. Worker：每通道一个 tokio task（同一 runtime），各自防抖、各自「最新优先」；全局并发上限 2（`tokio::sync::Semaphore`）；429 指数退避。
4. Server `dispatch/composed/mod.rs`：Typing 请求与 Translate **同时发出**，删掉「Rerank 回来再发 Translate」的串行链；按 lane 分派结果。
5. 测试：「Translate 不让 Typing 过期」「同通道新请求让旧请求过期」「Deep 不被打字打断」。

**验收**：长拼音 + 学习模式场景，识别结果到达 ≈ 单次请求耗时。
**提交**：`refactor(predict): 云端请求按打字/学习/深度三通道并行`

状态：已完成（2026-10-13，Linux 沙箱）。core 新增 `PredictionLane` 与按通道判过期，`Prediction` 带 `kind`（壳据此分派，不再看 `last_prediction_kind`）；Worker 三通道并行；Windows 组句时重排与翻译同时发出；429 改指数退避。core / predict（含用本机假服务的并行测试）/ platform / cli 测试全过，`qingjian-windows-server`（含测试）、tsf、settings 用 zig 做 C 编译器在 Linux 上 `cargo check --target x86_64-pc-windows-gnu` 通过。**与计划的偏离**：① `Question` 归 Typing 通道（问字与打字一样每键换，放 Deep 会被慢请求占住）；② Deep 通道不占「并发上限 2」的名额（Typing + Learning 合计 ≤ 2），避免一个慢的深度请求挡住打字。**未验证**：Windows 测试只编译未运行（沙箱无 Windows）、macOS 壳（仅一处判断条件，无法编译）、真实接口下的并行延迟（无密钥）。

---

## P3　本地纠错增强（约 2 天，纯本地、即时）

目标：网络断开时也尽量识别出来；AI 只做补强。

1. **分段修复（多处错误的长串）**：在 `crates/qingjian-core/src/correction/` 新增 `segment_repair.rs`。从左到右做最长切分；遇到切不动或残缺的位置，在该位置附近 ≤ 6 个字母的窗口里尝试 1 处编辑（换字母、相邻键、多一个、少一个、相邻颠倒），能接上合法音节就继续往右切。束搜索宽度 8，总编辑数上限 `ceil(len / 6)`，每处编辑按 `TypoCosts` 计代价。输出若干「完整切分 + 代价」交给现有的整句转换打分。适用 4–64 位，替代现在两处编辑「仅 7–24 位且整句不可用」的限制（旧 `two_edit_candidates` 保留做兜底）。
2. **颠倒专项**：音节内两个字母颠倒（`hoa→hao`、`zhnog→zhong`、`xian→xain` 反向）代价调低到与相邻键同级；同一音节内允许 1 次颠倒 + 1 次别的编辑。
3. **快速提示放宽**：`fast_local_intent_candidate` 从 ≥7 位放宽到 ≥4 位；4–6 位只允许 1 处编辑。
4. **「纠」标记**：`Candidate` 新增 `#[serde(default)] pub corrected: bool`（协议是 JSON，旧 DLL 兼容）。纠正产生的候选置 `true`。两套候选窗都要画：`crates/qingjian-render/src/renderer/item.rs`（qingjian 渲染器）和 `apps/windows/server/src/ui/candidates/row.rs` / `view.rs`（GDI），在候选文字后画一个小胶囊「纠」（底 `#FEF3C7`、字 `#B45309`、字号 = 候选注释字号、圆角 4）。
5. **第 1 位放置规则（本地）**，新增 `engine/correcting/placement.rs`，一处集中判断：
   - 原串「不像话」（`correction::unlikely_pinyin` 为真，或没有覆盖整串的精确候选）且纠正候选覆盖整串 → 纠正候选放第 1 位，标「纠」。
   - 原串本身合法且有覆盖整串的精确候选 → 纠正候选**最多放第 2 位**（防误纠），也标「纠」。
   - 个人敲错表里同一 (敲的, 要的) 已被接受 ≥ 2 次 → 视为高把握，放第 1 位。
   - 用户按回车原样上屏过的串 → 以后不再纠（沿用 Learner 的「不纠」记录）。
6. **学习闭环**：用户选中带「纠」的候选 → 写入个人敲错表（`FrequencyLearner` 已有）；选回原第 1 位 → 记一次「拒绝纠正」，同一串连续拒绝 2 次后该串不再自动放第 1 位。
7. 测试：`engine/tests/correction.rs` 加样本集里的典型例子（每类至少 3 条）；误纠回归用 300 条 clean 样本跑 `--replay`。

**验收**：本地 Top-1 ≥ 55%（120 条错拼）；clean 误纠 ≤ 1%；本地候选 P95 ≤ 3 ms（`qingjian-cli` 逐键计时）。
**提交**：`feat(correction): 长串多处错拼的分段修复与「纠」标记`

状态：已完成（2026-10-13，Linux 沙箱）。`correction/segment_repair.rs`（逐音节动态规划）+ `Candidate.corrected` + 两套渲染的「纠」胶囊 + `engine/correcting/placement.rs` 放置规则 + 拒绝纠正计数 + `fast_local_intent_candidate` 放宽到 ≥4 位；
core / learning / render / predict / platform / cli 测试全过，Windows server / tsf / settings 交叉 `cargo check --tests` 通过。本地数字见 `docs/notes/ai-latency-baseline.md`：typo Top1 65.8%（≥55% 满足）、clean 首选标「纠」0.3%（≤1% 满足）、新增 multi 类 Top1 1.0% → 14.9%、首选字符相似度 63.5% → 78.1%。
**与计划的偏离 / 没做到**：① 「本地候选 P95 ≤ 3 ms」只在 ≤12 位的输入上成立（P95 ≤ 8 ms，clean P95 1 ms）；20+ 位长串逐键平均 3.6 ms、最慢 6.9 ms，`multi` 查询 P95 21 ms，没有达标。② 计划第 2 条「同一音节内 1 次颠倒 + 1 次别的编辑」没做（分段修复每个音节只允许 1 处编辑；颠倒本身代价已降到与相邻键同级）。
③ 分段修复没用「束宽 8」：宽度 96，外加按代价上界（最便宜路径 + 3 nat）剪枝，否则 `multi` 样本召回太低；多处错整句精确命中率仍低，是因为被敲错的音节本身常是合法音节，要靠 P4 的 AI Resolve。④ 原串合法且有精确候选时，纠正候选以前被直接丢掉，现在按计划放第 2 位并标「纠」（`jineng` 的 近藤 就是这样）。⑤ 「拒绝纠正」只在纠正与原串候选同时在列表里时记（整串替换的纠正里没有原串候选可选回，仍靠回车原样上屏的 `record_raw`）。⑥ 只记同一串**连续**拒绝，选纠正候选会清零。
**未验证**：Windows 测试只编译未运行；两套渲染的「纠」胶囊没有在真机看过（沙箱没有系统字体，`qingjian-render` 的胶囊测试在沙箱里自动跳过，只在有字体的 macOS / Windows 上跑）；macOS 只接了数据通路（`Row.corrected`、位图渲染），AppKit 原生视图没画；Linux 壳（`apps/linux/server`）原本就缺 `Frame` 字段、编不过，只补了 `Candidate` 字段；真实 AI 链路不受影响但没有联网验证。

---

## P4　AI 智能识别 Resolve（约 2 天）

把 Rerank 和 Intent 合并成**一个请求**：同时做「错拼纠正」和「按上下文选字」。

**请求**（新增 `PredictionKind::Resolve`，Typing 通道，防抖 120 ms）
```json
{"letters":"wojintainquchifna","segmentation":"wo'jin'tain'qu'chi'fna",
 "before":"…光标前 ≤64 字…","after":"…光标后 ≤32 字…",
 "candidates":[{"i":1,"text":"我今天去吃饭","reading":"wo'jin'tian'qu'chi'fan","corrected":true},
               {"i":2,"text":"…","reading":"…"}]}
```
- `candidates` = 当前布局前 12 个本地候选（含 P3 产生的纠正候选），带读音。
- system prompt 放在 `crates/qingjian-predict/src/resolve/prompt.rs`，内容要点：只理解「当前这一串」想打什么，绝不续写；可以修正拼音（错键、颠倒、漏、多、边界）；结合前后文选同音字；必须严格输出 JSON；给 4 条少样本（错拼、颠倒、上下文同音、无需修改）。

**回复**（≤ 30 token）
```json
{"pinyin":"wojintianquchifan","pick":1}
```
- `pinyin`：用户本意的完整拼音（无需纠正就原样返回）；`pick`：选中的候选编号，没有合适的给 `0`。

**Core 校验**（`engine/prediction/resolve.rs`）
1. `pinyin == letters` → 只是选字：`pick` 必须在列表里，否则丢弃。
2. `pinyin != letters` → 编辑距离上限按长度（<10：2；10–15：3；16–39：4；≥40：5），且必须完整合法切分；然后**用纠正后的拼音重新查一次本地**（新增 `Engine::query_reading(&str) -> Vec<Candidate>`）。`pick` 指向的文字若出现在新结果前 9 个且读音完全一致 → 用它；否则用新结果第 1 个。这一步解决 R4。
3. 过期（序号不对、用户已翻页或移动高亮、组句已变）→ 丢弃。

**放置**（复用 P3 的 `placement.rs`）
- AI 结果与当前第 1 位不同，且通过校验 → 放第 1 位，标「纠」，原第 1 位降到第 2 位。
- 原串合法、AI 只是按上下文换了同音词 → 同样放第 1 位并标「纠」，但要求 `before` 或 `after` 非空（没有上下文时不改第 1 位，只在前 3 位内调整）。
- AI 与本地纠正结果一致 → 保持第 1 位，「纠」标记不变。

**触发**
- ≥ 4 个字母，或 ≥ 2 个音节；英文、双拼、注音、形码、问字、表达、私密输入一律不发。
- 跳过：单字且无上下文；本地首选与次选分差很大、没有纠正候选、也没有上下文。
- 缓存 key = (letters, before 的哈希, after 的哈希)。

**兼容**
- Windows 默认打字路径改用 Resolve；旧 `request_rerank` / `request_intent_suggestion` 保留（macOS 还在用，Mac 上验证前不动 macOS 壳）。
- 拼音行右侧提示 + Tab 采用的旧行为保留：当 AI 结果没能放第 1 位（把握不够）时仍用提示位。

**测试**：解析器（合法 / 多字段 / 编号越界 / 读音不一致 / 编辑距离超限）；`query_reading`；放置规则；Server 端到端：`kaifazhes`、`nihoomma`、`wojintainquchifna`、上下文同音各一条（Mock 回复）。
**验收**：`--ai-bench` 本地 + AI Top-1 ≥ 80%、Top-3 ≥ 90%；clean 误纠 ≤ 1%；context Top-1 ≥ 85%；P50 ≤ 700 ms。
**提交**：`feat(predict): 合并重排与意图纠错为上下文感知的 Resolve 请求`

状态：已完成代码与测试，**验收未全部达标**（2026-10-13，Linux 沙箱）。`PredictionKind::Resolve` + `request_resolve` / `query_reading` / 校验 / 放置 / `apply_resolved`（core）、`resolve/` 解析与提示词（predict，防抖 120 ms，缓存 key 只含 kind / letters / before / after）、Windows Server recompose 改发 Resolve、`--ai-bench` 改测 Resolve；core 369 / predict 52 + 3 / cli 等测试全过，Windows server / tsf / settings 交叉 `cargo check --tests` 通过。本地 `--local-bench` 回归：clean Top1 97.0%、标「纠」0.3%，与 P3 一致。
**与计划的偏离**：① 选中候选带读音时以候选自己的读音为准、不用模型回的拼音（小模型常把拼音抄错，实测后改的）；② 无上下文、无纠正候选且首选读音恰好等于键入串时视为「本地把握大」不发请求；<4 字母的短串只有带上下文才发；③ `after` 暂时恒为空（P5 才读光标后文）；④ 选了 AI 纠正候选上屏时整段 scope 一起吃掉，不学习敲错对；⑤ 疑似敲错时被动翻译延后到 Resolve 回来再发，避免拿错串翻译。
**验收数字**（沙箱真实接口，每类 10 条、1 轮，详见 `docs/notes/ai-latency-baseline.md`）：clean 误纠 0 / 10（满足 ≤ 1%）；typo +AI Top1 90%（持平本地）；context Top1 30% → 60%（**未达 85%**）；multi Top1 10%，无有效建议 44–56%（**未达标**，8B / 14B 回的拼音自己就错）；本地 + AI 总体 Top1 ≥ 80% / Top3 ≥ 90% 也未达到。P50 在沙箱约 1.0–1.6 s，但沙箱到接口单次往返就 0.5 s，**「≤ 700 ms」无法在此验证**。校验与放置是可靠的（误纠 0%），缺的是模型质量，**模型选型交给 P6**（要试更强的打字模型）。
**未验证**：Windows 新增 / 改写的测试只编译、没运行；「纠」标记在 Windows 真机上没看过；Mac 壳没动；真实用户网络下的延迟；`after` 为空时上下文只有光标前文。

---

## P5　读取光标后文（约 0.5–1 天，要换 TSF DLL）

1. `apps/windows/tsf/src/com/edit/surrounding.rs`：新增 `text_after_caret`：克隆插入点 range，`ShiftEnd(ec, +32)` 后 `GetText`；读不到返回 `None`。私密输入框不读（沿用现有判断）。
2. 协议 `ClientMessage::Surrounding` 加 `#[serde(default)] after: String`（JSON，旧 DLL / 旧 Server 互通）；`PROTOCOL_VERSION` 只有在本版本周期还没升过时才 +1（看 `git log -p crates/qingjian-platform/src/protocol/mod.rs`）。Linux 插件写死了版本号，升了要一起改。
3. Server：`surrounding_before` 改为 `surrounding: Option<SurroundingText>`；`rerank_surrounding()` 返回真实 `after`；进入私密 / 组句结束时清空（沿用现有逻辑）。
4. 测试：`apps/windows/tsf/tests/` 里补 `after` 读取用例；`apps/windows/server/tests/pipe.rs` 补协议兼容用例（旧消息无 `after` 字段也能解析）。
5. 部署：单独构建、单独替换 DLL，保留旧 DLL 回滚；部署说明写进 `docs/notes/`。

**验收**：在记事本里光标放在一句话中间敲拼音，日志显示 `after` 非空（只记长度，不记内容）。
**提交**：`feat(tsf): 组句时读取光标后文供智能识别使用`

状态：已完成代码与测试，**真机验收未做**（2026-10-13，Linux 沙箱）。`text_after_caret`（TSF，克隆插入点 range + `ShiftEnd(+32)`）、`ClientMessage::Surrounding` 的 `#[serde(default)] after`、Server 的 `surrounding: Option<SurroundingText>` 与 `rerank_surrounding()` 返回真实 `after`、Surrounding 迟到时重发 Resolve（原来重发的是旧 Rerank）。platform 兼容用例（旧消息无 `after` 能解析、新消息旧 Server 能忽略）与 Server 端到端用例（`after` 进入 Resolve 请求）通过，core / predict / platform / cli 等测试全过，Windows server / tsf / settings 交叉 `cargo check --tests` 通过。
**与计划的偏离**：**没有升 `PROTOCOL_VERSION`**（仍是 7）。计划写「本周期没升过才 +1」，但新字段是可选的，旧 DLL / 旧 Server 互通；升版本会让 `SessionOpened` 挡住没换 DLL 的进程，也会让「换回旧 DLL 回滚」断连，所以不升。互通矩阵与部署 / 回滚见 `docs/notes/tsf-surrounding-after.md`。Linux 插件写死的版本号因此没动。另：前后文任一非空就发 Surrounding（以前只有前文才发，光标在开头时 `after` 会丢）。
**未验证**：TSF 的 `ShiftEnd` / `GetText` 在真实宿主（记事本、Chrome、Word）里能读到多少——沙箱没有 Windows，TSF 只编译；`after` 非空的日志验收（记事本里光标放句中）需要在真机做；DLL 在被占用时的替换流程只写了说明、没演练。

---

## P6　选定打字模型（约 0.5 天）

用 P1 的 `--ai-bench` + P4 的 Resolve 提示词实测，候选（均为 SiliconFlow 当前在售型号，关思考或本身无思考）：
- `Qwen/Qwen3-8B`（`enable_thinking=false`）
- `Qwen/Qwen2.5-7B-Instruct`
- `Qwen/Qwen3-30B-A3B-Instruct-2507`（MoE，激活约 3.3B，无思考版）
- `Qwen/Qwen3-14B`（`enable_thinking=false`，作对照）

**选择规则**：Resolve 解析通过率 ≥ 95%，且错拼 Top-1 不低于最好成绩 5 个百分点以内的模型中，选 **P95 最低**的。每个模型跑 3 轮，取中位数。结果写 `docs/notes/ai-model-bench.md`，把胜出者写进配置模板的 `typing_model` 默认值。`panel_model` 保持 `Qwen/Qwen3-14B`。

**提交**：`chore(predict): 按实测结果设定默认打字模型`

---

## P7　面板渲染基础（约 2 天）

两个面板都改用仓库已有的 `qingjian-render`（tiny-skia + cosmic-text，候选窗在 `renderer = "qingjian"` 时已经在用），贴图走 `apps/windows/server/src/ui/layered::present`。不再用 GDI 画面板。

新增 `crates/qingjian-render/src/panel/`：
- `canvas.rs`：圆角矩形、描边、阴影、胶囊、分割线、可换行文本（返回高度）、文本测宽。
- `icons.rs`：Lucide 图标（ISC 许可，可商用）的 SVG path 字符串 + 一个小型 path 解析器 → `tiny_skia::Path`，统一 1.75 px 描边。需要的图标：`pin, minus, x, volume-2, copy, star, mic, message-square, sparkles, file-text, scan-text, smile, target, lightbulb, chevron-down, chevron-right, plus, settings, paperclip, image, send, refresh-cw, circle-alert, loader-circle, circle-check, ellipsis, book-plus, message-circle`。
- `avatar.rs`：内嵌 PNG（工作区已有 `png = 0.18.1`）解码成 Pixmap，圆形裁切。头像素材**已提供**：`assets/ui/avatars/avatar_a_boy{,@2x,@3x}.png`、`avatar_b_girl{,@2x,@3x}.png`（40/80/120 px，圆形透明 PNG，见该目录 README），按 DPI 选图，不要再做圆形裁切。
- `hitmap.rs`：**绘制时同步登记点击区域** `Vec<(Rect, Hit)>`，窗口过程只查表，彻底消除 U3。
- `tokens.rs`：设计 token（见下表）。

离线预览与对比：
- `examples/preview.rs` 增加 `--panel companion-tab0|tab1|tab2|loading|streaming|error|empty|ai-chat`，按 420 px @2x 输出 PNG。
- 新增 `tools/ui-diff/diff.py`：把预览图和 `docs/design/reference/` 里对应的参考裁图缩放到同宽，输出左右并排图和 50% 叠加图。参考图是 AI 生成的样稿，字形不可能逐像素一致；验收按**布局 ±2 px、颜色 token、元素齐全**。

设计 token（spec 为准，标签/卡片色从严格参考图取样）：

| token | 值 | 用途 |
|---|---|---|
| primary | `#2563EB` | 主色、选中标签文字、主按钮 |
| selected-bg | `#EBF2FF` | 选中标签背景（边框 primary 1 px） |
| tabbar-bg | `#EEF2F7` | 未选中标签所在的分段条底色 |
| card-bg | `#F8FAFC` | 通用卡片 |
| card-blue / card-purple | `#F3F7FE` / `#F9F7FE` | 「意思」「使用场景」卡片底色 |
| border | `#E5E7EB` | 边框、分割线 |
| text | `#111827` | 主文字 |
| text-2 | `#6B7280` | 次文字、中文翻译 |
| tag-orange | 底 `#FEF3C7` / 字 `#B45309` | 自然口语、更有力度、序号 3 |
| tag-green | 底 `#DCFCE7` / 字 `#15803D` | 鼓励支持、常用表达、日常口语 |
| tag-blue | 底 `#DBEAFE` / 字 `#1D4ED8` | 温和安慰、等级胶囊 |
| icon-chip | 蓝 `#DBEAFE`、绿 `#DCFCE7`、紫 `#EDE9FE` | 区块图标底 |
| title | 蓝 `#2563EB`、绿 `#16A34A`、紫 `#7C3AED` | 意思 / 语气 / 使用场景 标题 |
| badge | 1 `#2563EB`、2 `#14B8A6`、3 `#F59E0B` | 替换表达序号圆点 |
| error | 底 `#FEF2F2` / 边 `#FECACA` / 字 `#DC2626` | 错误状态 |
| radius | 卡片 10 px、标签与按钮 8 px、胶囊 6 px | |
| 窗口宽度 | 420 px | |

---

## P8　学习助手面板重构（约 3 天）

### 8.1 冲突规则
严格参考图优先；规范总览只补充严格图里没有的状态（加载、流式步骤、空状态、头部菜单）和 token。具体取舍：
- 底部按钮：**收藏 / 练一句 / 问 AI（蓝色主按钮）**（严格图）。复制放在每个例句卡片的复制图标上。
- 标签带图标（严格图）；第 2 个区块标题用「语气」（严格图）。
- 场景胶囊包含「更多场景 ▾」（严格图）。

### 8.2 布局（420 px 宽，自严格图量取，±2 px 内微调）

| 区块 | 尺寸 |
|---|---|
| 头部 | 高 34：绿色 logo 14 px + 「青简 · 英语学习助手」13 px + BETA 胶囊；右侧 pin / minus / x 图标 16 px，间距 14 |
| Hero | 中文 14 px；英文 24 px 粗体 primary + 喇叭图标；标签胶囊高 20，右侧等级胶囊（B1-B2） |
| 标签栏 | 高 32，分段条底色 tabbar-bg，选中项白底改为 selected-bg + primary 描边，三项等宽，项间 1 px 竖分隔 |
| Tab0 区块卡 | 每张高约 52，左侧 28 px 圆角图标底，标题 13 px 彩色粗体，正文 13 px；卡间距 6 |
| 自然例句 | 标题 13 px primary；例句卡高约 54：英文 13 px（高亮短语加粗）+ 中文 12 px text-2，右侧喇叭 + 圆形复制按钮 |
| 搭配与语法细节 | 折叠行高 38，左 chevron-down，右 chevron-right，标题带「（3 项）」；展开后列出 3 条 |
| 底部 | 高 52，顶部分割线；三按钮高 34，宽度比例约 0.27 / 0.31 / 0.42 |
| 总高 | 约 564，内容区可滚动，细滚动条 |

Tab1：标题行「3 个更地道的替换表达」13 px 粗体 + 右侧灰字「不同语气，适用不同场景」11 px；每张卡：序号圆点 22 px、表达 14 px 粗体 + 喇叭、右上标签胶囊、中文释义、两条「语气 / 区别」小圆点列表 11.5 px、底部浅色例句框（英文 + 中文 + 复制图标）。

Tab2：场景胶囊行（选中项实心 primary 白字，其余白底描边，最后是文字按钮「更多场景 ▾」）；场景卡标题「场景 1：朋友聊天（鼓励安慰）」带图标；A 左侧头像 + 浅灰蓝气泡、喇叭在右；B 右侧头像 + 浅绿气泡（目标表达加粗）、喇叭在左，气泡内小号「B」角标；底部「表达亮点」卡带灯泡图标 + 3 条要点。

### 8.3 数据与生成：流式 JSONL 代替 Markdown
每个标签让模型逐行输出 JSON，一行一个区块，UI 每收到一行就填一个卡片（这正是规范图「流式逐步显示」的效果）：

```
Tab0:
{"k":"header","tags":["自然口语","鼓励支持","常用表达"],"level":"B1-B2"}
{"k":"meaning","text":"尽你所能；尽力就好。"}
{"k":"tone","text":"鼓励、支持，强调努力，不要求结果一定完美。"}
{"k":"usage","text":"用于安慰或鼓励别人，例如考试、工作、比赛等场景。"}
{"k":"example","en":"Just do your best. That's all you can do.","zh":"尽力就好，你能做的就是把自己能做的做到最好。","hl":"Just do your best"}
{"k":"detail","title":"…","body":"…"}   × 3
Tab1:
{"k":"alt","i":1,"expr":"Give it your best shot","zh":"尽力一试；试试看。","tag":"日常口语","tone":"鼓励、轻松","diff":"强调尝试，不确定能否成功时更自然。","ex_en":"…","ex_zh":"…"}   × 3
Tab2（每个场景一次请求）:
{"k":"scene","title":"朋友聊天（鼓励安慰）"}
{"k":"turn","sp":"A","en":"…","zh":"…","hl":""}   × 3–4
{"k":"point","text":"…"}   × 3
```

- 新文件 `crates/qingjian-predict/src/companion/`：`prompt.rs`（三个标签和四个场景的 system prompt，各带一条「Do your best」少样本示例）、`jsonl.rs`（按行缓冲，坏行丢弃并计数）、`validate.rs`（英文字段必须是英文；标签 ≤ 3 个、每个 ≤ 4 个汉字；等级匹配 `^(A1|A2|B1|B2|C1|C2)(-(A1|A2|B1|B2|C1|C2))?$`）。
- 请求参数：模型用 `panel_model`（默认 Qwen/Qwen3-14B），关思考、温度 0.5、`max_tokens` Tab0 600 / Tab1 700 / Tab2 700。
- 按需生成：打开时只请求 Tab0；Tab0 的 header 行到达后在后台预取 Tab1；Tab2 在第一次点击或空闲 2 s 后请求，场景胶囊各自请求、各自缓存。
- 缓存：`(中文, 英文, tab, 场景)` → 结构化结果，LRU 200 条，存到用户目录 `companion-cache.json`；再次打开秒开。
- 重绘节流：流式期间最多 30 fps。

### 8.4 状态
- 加载（spec_4 / strict_5）：转圈图标 + 「正在生成语感解析…」+ 骨架条 + 「正在分析表达的含义、语气和使用场景，预计 3-5 秒…」。
- 流式步骤（spec_5）：已获取基础释义 / 正在分析语气和场景 / 正在生成例句 / 正在整理搭配信息，完成的打勾。
- 错误（spec_6 / strict_5）：红色感叹图标 + 「生成失败」+ 原因 + 「重新生成」按钮（只重试当前标签）。
- 空状态（spec_7）：文档图标 + 「点击上方选项卡查看内容」+ 说明文字。
- 头部菜单（spec_9）：点 pin 切换置顶；右键头部弹出「置顶窗口 / 取消置顶 / 最小化 / 关闭」。

### 8.5 动作
- 喇叭：新增 `ui/tts.rs`，用 SAPI `ISpVoice::Speak`（`windows` crate 开 `Win32_Media_Speech` 特性），异步标志 + 独立 COM 线程，en-US 语音。
- 复制：沿用 `copy_to_clipboard`，提示「已复制」1.5 s 后消失。
- 收藏：沿用 `LearningVault::save_expression`。
- 练一句：沿用 `StartDrillSession`。
- 问 AI：打开 AI 问答面板，主题设为当前表达，并预填欢迎卡片。

### 8.6 代码组织
`ui/companion.rs` 拆成 `ui/companion/{mod.rs, model.rs, layout_header.rs, tab_nuance.rs, tab_alternatives.rs, tab_dialogue.rs, states.rs, input.rs}`，每个文件 < 500 行；`dispatch/mod.rs` 里的三路请求挪到 `dispatch/companion/mod.rs`。

验收：七种状态的预览 PNG 与参考裁图并排对比通过；钉住时打字不覆盖内容（保留现有行为）；所有按钮点击命中区与绘制一致（hitmap 单测）。

---

## P9　AI 深度问答面板（约 2–3 天）

布局（strict_4）：默认 720 × 520，可最大化（保留现有逻辑）。
- 左侧栏 168 px：logo +「青简 AI 聊天」+ 右侧「+」；「+ 新对话」按钮；会话列表（图标 + 标题 + 「N条消息 · MM/DD」，选中项 selected-bg）；底部「设置」。
- 主区顶栏：「当前主题：Do your best」+ BETA 胶囊；右侧「清空会话」。
- 欢迎卡片：「👋 你好！我可以围绕"Do your best"帮你：」+ 4 条要点（解释更地道的用法和语气区别 / 给出更多真实的例句与场景 / 为你设计 IELTS 口语练习题 / 帮你纠正你自己说的句子）。严格图里有 👋，cosmic-text 支持彩色 emoji，按严格图保留。
- 建议胶囊 4 个，由主题套模板生成：什么时候用 X？/ 请给我 IELTS 口语题 / X 和 <Tab1 第一个替换表达> 有什么区别？/ 美国年轻人会怎么说？
- 输入框：占位「输入你的问题……（Enter 发送，Shift+Enter 换行）」，右侧回形针、截图图标、蓝色方形发送键（流式时变停止键）。
- 去掉所有 emoji 图标按钮（📌 ✎ 🌐 ■ ↑），换成矢量图标；去掉「Qwen3 14B · 极速高性价比专享推理」这类营销文案。

数据：
- 新增 `crates/qingjian-learning/src/chat_history.rs`：每个会话一个 JSON（id、标题、主题、更新时间、消息），存用户目录 `ai-chat/`；侧栏列最近 20 个。
- 标题 = 主题；没有主题时取首个问题前 12 个字。
- 模型用 `panel_model`；关思考为默认；「深度」开关打开时才允许思考，思考内容折叠显示。
- 流式重绘 ≤ 30 fps；Markdown 只重新解析最后一个气泡。

代码：`ui/ai_assistant.rs` 拆成 `ui/ai_assistant/{mod.rs, sidebar.rs, topic_bar.rs, messages.rs, composer.rs, input.rs}`。

验收：ai-chat 预览 PNG 与 strict_4 并排通过；新建 / 切换 / 清空会话可用，重启后会话还在；流式期间 CPU 不再整窗满载。

---

## P10　回归、文档、部署（约 0.5 天）

每个 Phase 提交前都跑：
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`（非 Apple 平台排除 `qingjian-macos`）
3. `cargo test -p qingjian-core --lib`、`-p qingjian-predict --lib`、`-p qingjian-render`
4. Windows 上：`cargo test -p qingjian-windows-server`、`cargo check -p qingjian-windows-server`
5. `git diff --check`
6. `qingjian-cli --replay`（冻结数据）与 `--ai-bench`（输入线 Phase）
7. 部署前备份 `E:\Qingjian\qingjian-server.exe`，核对 Release 与线上 SHA-256；非 P5 不换 TSF DLL
8. 同步 `docs/notes/crate-notes.md`、`CHANGELOG.md`、`docs/plan/ai-current-input-optimization.md` 的状态

---

## Sonnet 执行规范（必须遵守）

**必读**：`CLAUDE.md`、`docs/contributing.md`、本文件。三者冲突时：用户在本文件里锁定的产品决定 > `docs/contributing.md` > 其它。

1. **顺序**：P1 → P2 → P3 → P4 → P5 → P6，然后 P7 → P8 → P9，最后 P10。不跳步；某个 Phase 卡住超过两次尝试，停下来写清卡点，再问用户。
2. **一个 Phase 一个提交**（P1 可拆两个）。提交信息用 Conventional Commits，类型 / 范围英文小写，说明用中文，正文写「为什么」；**不加 AI 署名**。不要 `--no-verify` 跳过钩子。
3. **本地优先**：任何改动不得让本地候选变慢（P95 ≤ 3 ms）；网络失败、超时、坏回复一律静默降级，不弹窗。
4. **AI 不能凭空造词**：上屏候选必须来自本地词库查询（Resolve 的纠正结果也要经 `query_reading` 落到本地候选上）。
5. **隐私**：不把用户输入、光标前后文写进日志（只记长度和耗时）；私密输入框不发任何请求；API 密钥不打印。
6. **代码组织**：单文件 ≤ 800 行（目标 500）；改到超长文件时顺手按职责拆目录；新文件写 `//!` 文件头；错误用 `thiserror`；依赖用 `cargo add` 并提到根 `[workspace.dependencies]`。
7. **界面**：每个界面 Phase 先在 `examples/preview.rs` 出预览 PNG，用 `tools/ui-diff/diff.py` 和 `docs/design/reference/` 并排对比，自查通过后再接到 Windows 窗口；把对比图路径写进提交正文。
8. **验证边界**：Windows Server 和 TSF 只能在 Windows 机器上编译和实测；在 Linux / 云端只跑 core / predict / render 测试和预览。不要声称在线效果已验证，除非真的跑了 `--ai-bench`。
9. **每个 Phase 完成后**在本文件对应小节末尾追加一行「状态：已完成 / commit / 关键数字」，并推送到 `my-github` 远端的 `owner/v0.1.4-english-learning`。
10. **不确定就问**：产品行为不在本文件里的，先问用户，不要自行发挥。

## 给 Sonnet 的启动指令（复制即可）

> 你在青简输入法仓库的 `source` 目录工作，分支 `owner/v0.1.4-english-learning`。先完整阅读 `CLAUDE.md`、`docs/contributing.md` 和 `docs/plan/ai-panel-rebuild.md`，严格遵守其中的「已锁定的产品决定」和「Sonnet 执行规范」。从 P1 开始，按顺序逐个 Phase 执行：每个 Phase 先列出要改的文件和测试，再动手；完成后跑该 Phase 的测试与验收，提交（中文 Conventional Commits，无 AI 署名），在计划文件里记录状态，推送到 `my-github`。遇到计划里没写的产品行为，停下来问我。
