# 青简 AI 联想 / 智能纠错 / 学习助手面板 / AI 问答面板：重构执行计划

分支：`owner/v0.1.4-english-learning`（基线 commit `d8ebcb0`）
参考图：`docs/design/reference/`（strict_* 来自《严格参考图》，spec_* 来自《界面规范总览》）
执行者：Sonnet。每个 Phase 一个独立提交，测试不过不进下一步。

---

## 0. 结论：为什么「慢」和「不智能」（含代码证据）

### 0.1 AI 联想 / 纠错

| # | 根因 | 证据 | 影响 |
|---|---|---|---|
| R1 | **Qwen3 思考模式没关**。SiliconFlow 上的 Qwen3-14B 默认 `enable_thinking=true`，代码只给智谱发关思考参数 | `crates/qingjian-predict/src/chat_client.rs:328` `ThinkingSwitch::for_url` 只认 `bigmodel.cn / z.ai`；`stream_client.rs:33` 请求体根本没有思考开关字段 | 每次请求先「想」再答：延迟翻倍。Intent 的 `max_tokens=96` 被思考吃光，所以返回**空建议**（plan 文档里 `kaifazhes` 烟测就是这样） |
| R2 | **一条通道串行**。Engine 只有一个 `prediction_sequence` 和 `last_prediction_kind`，Worker 只有一个循环，新请求打断旧请求 | `engine/prediction/mod.rs:601`；`worker.rs` 单 loop；`dispatch/composed/mod.rs:131-153` 先 Rerank，回来后才 Translate，然后才 Intent | 实测 2164 + 676 + 1118 ms ≈ 4 s 才出纠错建议 |
| R3 | **Rerank 输出太大、太脆**。要求模型逐字回显全部 27 个候选的完整排列，最多 512 token；漏一个、错一个字就整份丢弃 | `prompt.rs:173` `RERANK_SYSTEM_PROMPT`；`prompt.rs:763` 校验 | 单次约 2.1 s，且失败率高 → 用户感觉 AI 没起作用 |
| R4 | **Intent 只能从「错拼串」的前 9 个本地候选里挑**。如果正确词不在这 9 个里，AI 修不了 | `engine/prediction/mod.rs:81-166`、`resolve_intent_candidate`（:254） | 真正的错拼大多救不回来 |
| R5 | 触发面窄：只认 7–64 位、且整串没有精确读音匹配；3–6 位短错拼完全没有 AI 兜底 | 同上 | 「不智能」 |
| R6 | **AI 联想没有可见输出**。默认打字路径只做 Rerank（只改顺序），云端词和整句补全都关了 | `engine/prediction/mod.rs:372` | 用户看不到任何「AI 联想」 |
| R7 | 冷连接：空闲 90 s 后连接池释放，下一次要重新 TLS 握手；温度 0.3（应为 0） | `chat_client.rs` `http_client()`、`TEMPERATURE` | 每次停顿后的首个请求多 200–400 ms |

### 0.2 学习助手面板（companion）与 AI 问答面板（ai_assistant）

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
| 本地候选出现 | ≤ 2 ms | 不变（任何改动不得变慢） |
| 本地纠错提示出现 | 仅 ≥7 位 | 3 位以上，≤ 5 ms |
| AI 纠错建议出现（停顿后） | ≈ 4 s，常为空 | P50 ≤ 700 ms，P95 ≤ 1500 ms，空回复率 < 5% |
| AI 重排成功应用率 | 未统计，估计低 | ≥ 90%（解析通过率） |
| 误纠率（正确拼音被改） | 未统计 | ≤ 1%（200 条正确样本） |
| 纠错召回（50 条真实错拼） | 未统计 | ≥ 70% Top-1，≥ 85% Top-3 |
| 学习助手首屏内容 | 首字等待长 | TTFT ≤ 800 ms，首个卡片 ≤ 1.2 s；再次打开同一表达 ≤ 50 ms（缓存） |
| 面板外观 | 不一致 | 与参考图逐区块对齐：布局 ±2 px、颜色用下方 token、所有图标为矢量 |

---

## 2. 执行顺序总览

```
P0 基线度量 ─► P1 立即提速 ─► P2 多通道并行 ─► P3 重排瘦身 ─► P4 纠错 v2 ─► P5 AI 联想（待用户定形态）
                               │
P6 面板渲染基础 ─► P7 学习助手面板 ─► P8 AI 问答面板 ─► P9 回归、文档、部署
```
P1 改动最小、收益最大，必须最先做。P6–P8（界面）与 P2–P5（输入）互不依赖，可以分两条分支并行。

---

## P0　基线度量（约 0.5 天）

1. 在 `chat_client.rs` 和 `stream_client.rs` 已有的 tracing 里补齐字段：`kind`、`lane`、`ttft_ms`、`total_ms`、`output_tokens`（从 `usage` 读）、`parse_ok`。
2. 新增 `apps/cli` 子命令 `--ai-bench <samples.tsv>`：对固定样本逐条调用真实接口，输出每种 kind 的 P50/P95、空回复率、解析通过率。
3. 建立 `apps/cli/fixtures/ai_typing_samples.tsv`：
   - 50 条错拼：`kaifazhes→开发者`、`nihoomma→你好吗`、`zhngguo→中国`、`xainzai→现在`、`gognzuo→工作`、`shuohua→说话`（正确，做误纠对照）等；
   - 200 条正确拼音（从 `docs/plan/model-eval.md` 的冻结集里抽），用来算误纠率。
4. 记录基线到 `docs/notes/ai-latency-baseline.md`。

验收：能一条命令复现基线数字；不记录用户真实输入。

---

## P1　立即提速：关掉思考 + 小参数（约 0.5 天，最先做）

改动：
1. `chat_client.rs`：`ThinkingSwitch` 新增 `EnableThinkingFlag`，对 `siliconflow.cn`、`siliconflow.com`、`dashscope.aliyuncs.com` 发 `"enable_thinking": false`（并移除 `reasoning_effort`）。
2. **打字类请求（Rerank / Intent / Translate / 以后的 Complete）一律强制关思考**，不受配置 `reasoning_effort` 影响；只有 DeepLearn / Compare / AI 聊天的「深度」模式才允许思考。
3. `stream_client.rs`：`ChatCompletionStreamRequest` 加 `#[serde(skip_serializing_if = "Option::is_none")] enable_thinking: Option<bool>`，按同一套 host 规则填 `Some(false)`。
4. 温度：Rerank / Intent `0.0`，Translate `0.3`，面板 `0.5–0.7`。
5. 连接预热：Server 启动时、以及组句开始且距上次请求 > 45 s 时，后台发一个 `max_tokens=1` 的请求保温；`pool_idle_timeout` 提到 300 s。
6. 单测：三种 host 的请求体快照（含/不含 `enable_thinking`）。

验收：`--ai-bench` 上 Intent 空回复率 < 5%，Intent P50 较基线下降 ≥ 40%。

---

## P2　多通道并行（约 1–1.5 天）

1. 新增 `qingjian_core::PredictionLane { Typing, Learning, Deep }`，`PredictionKind::lane()` 映射：
   - Typing：Rerank、Intent、Complete（P5 新增）
   - Learning：Translate
   - Deep：DeepLearn、Compare、Question
2. Engine：`prediction_sequence: [u64; 3]`、`last_kind: [PredictionKind; 3]`，`poll_prediction` 按通道判断过期；Prediction 结构带 `lane`。
3. Worker：每个通道一个 tokio task（同一 runtime），各自的防抖与「最新优先」；全局并发上限 2（`Semaphore`），429 指数退避。
4. Server `dispatch/composed/mod.rs`：Rerank（或 Intent）与 Translate **同时发出**，删掉「Rerank 回来再发 Translate」的串行链；`poll_prediction` 按 lane 分派。
5. 测试：「Translate 不会让 Rerank 过期」「同一通道新请求让旧请求过期」「Deep 通道不被打字打断」。

验收：截图场景（长拼音 + 学习模式）纠错建议到达时间 ≈ 单次 Intent 耗时，不再是三次之和。

---

## P3　重排瘦身 Rerank v2（约 0.5 天）

1. 请求：候选编号 `1..N`（N ≤ 18，即前两页），附读音；Prompt 只要求返回**最多 3 个最该排前面的编号**：`{"pick":[4,1]}`。
2. Core 新增 `CandidateLayout::promote_indices(&[usize])`：被选中的按顺序提前，其余保持原序。输出约 10 token，不再整份回显。
3. 只在「确实有歧义」时请求：本地首选与次选分差小于阈值、或有光标前文、或 ≥ 2 音节；单字和高置信精确匹配不请求（省请求、省限流）。
4. 缓存 key 加入光标前文的哈希。
5. 保留现有规则：用户已翻页 / 移动高亮时不应用。

验收：Rerank 解析通过率 ≥ 90%，P50 ≤ 600 ms；冻结回放 Top-1 不下降。

---

## P4　智能纠错 v2（约 1.5–2 天）

本地（即时）：
1. `fast_local_intent_candidate` 的触发从 ≥7 位放宽到 ≥3 位，配合 `correction/costs.rs` 的键盘邻键代价；短串只允许 1 处编辑。
2. 纠错候选在候选窗里带一个小「纠」角标；拼音行用已有的错误标记高亮被改的字母。
3. 用户采用纠错后写入个人敲错表（`qingjian-learning` 的 `FrequencyLearner` 已有），下次同样的错拼由本地直接解决。

AI（停顿后）：
4. Intent v2 回复：`{"corrected_pinyin":"kaifazhe","pick":2}`（`pick` 可选）。
5. Core 校验编辑距离 + 完整合法切分后，**用纠正后的拼音重新做一次本地查询**（新增 `Engine::query_reading(&str) -> Vec<Candidate>`），在新结果前 9 个里：模型给的 `pick` 读音完全一致就用它，否则用新查询的首选。解决 R4：正确词不在错拼串候选里也能救回来。
6. 触发放宽到 ≥5 位：本地纠错无结果，或整串无法切分。英文、双拼、注音、形码、私密输入仍然排除。
7. 展示（待用户决定，见文末决策 2）：默认方案 = 本地与 AI 一致或 AI 结果通过校验时，把纠正结果放到第 1 位并带「纠」角标（空格即上屏）；同时保留拼音行右侧提示和 Tab 采用。

验收：50 条错拼 Top-1 ≥ 70%；200 条正确拼音误纠 ≤ 1%；`nihoomma`、`kaifazhes` 端到端用例通过。

---

## P5　AI 联想（约 1 天，形态待用户决定）

推荐方案 A：
1. 新增 `PredictionKind::Complete`（与旧 Compose 分开）：≥ 2 音节且停顿 300 ms 后请求，返回 1 条续写，≤ 8 个汉字，必须以当前首选开头。
2. 显示为拼音行右侧的灰色续写提示，Tab 采用；本地校验：不得含拼音字母、标点不超过 1 个、不与首选完全相同。
3. 本地候选明显不足时（首页没有覆盖整串的候选），云端词补进第 8、9 格，带云朵标记（沿用 `slots` 配置）。
4. 所有 Complete 走 Typing 通道，和 Rerank / Intent 共享「最新优先」。

备选 B：保持「只重排、不续写」现状（当前 plan 文档的原则）。
备选 C：上屏后联想（候选窗显示 3 个下文词，本地 bigram 先出、AI 再细化）。

---

## P6　面板渲染基础（约 2 天）

两个面板都改用仓库已有的 `qingjian-render`（tiny-skia + cosmic-text，候选窗在 `renderer = "qingjian"` 时已经在用），贴图走 `apps/windows/server/src/ui/layered::present`。不再用 GDI 画面板。

新增 `crates/qingjian-render/src/panel/`：
- `canvas.rs`：圆角矩形、描边、阴影、胶囊、分割线、可换行文本（返回高度）、文本测宽。
- `icons.rs`：Lucide 图标（ISC 许可，可商用）的 SVG path 字符串 + 一个小型 path 解析器 → `tiny_skia::Path`，统一 1.75 px 描边。需要的图标：`pin, minus, x, volume-2, copy, star, mic, message-square, sparkles, file-text, scan-text, smile, target, lightbulb, chevron-down, chevron-right, plus, settings, paperclip, image, send, refresh-cw, circle-alert, loader-circle, circle-check, ellipsis, book-plus, message-circle`。
- `avatar.rs`：内嵌 PNG（工作区已有 `png = 0.18.1`）解码成 Pixmap，圆形裁切。需要 2 个卡通头像素材（A 男生、B 女生，96×96 @2x），放 `assets/ui/avatars/`，附 README 写来源与许可。
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

## P7　学习助手面板重构（约 3 天）

### 7.1 冲突规则
严格参考图优先；规范总览只补充严格图里没有的状态（加载、流式步骤、空状态、头部菜单）和 token。具体取舍：
- 底部按钮：**收藏 / 练一句 / 问 AI（蓝色主按钮）**（严格图）。复制放在每个例句卡片的复制图标上。
- 标签带图标（严格图）；第 2 个区块标题用「语气」（严格图）。
- 场景胶囊包含「更多场景 ▾」（严格图）。

### 7.2 布局（420 px 宽，自严格图量取，±2 px 内微调）

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

### 7.3 数据与生成：流式 JSONL 代替 Markdown
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
- 请求参数：关思考、温度 0.5、`max_tokens` Tab0 600 / Tab1 700 / Tab2 700。
- 按需生成：打开时只请求 Tab0；Tab0 的 header 行到达后在后台预取 Tab1；Tab2 在第一次点击或空闲 2 s 后请求，场景胶囊各自请求、各自缓存。
- 缓存：`(中文, 英文, tab, 场景)` → 结构化结果，LRU 200 条，存到用户目录 `companion-cache.json`；再次打开秒开。
- 重绘节流：流式期间最多 30 fps。

### 7.4 状态
- 加载（spec_4 / strict_5）：转圈图标 + 「正在生成语感解析…」+ 骨架条 + 「正在分析表达的含义、语气和使用场景，预计 3-5 秒…」。
- 流式步骤（spec_5）：已获取基础释义 / 正在分析语气和场景 / 正在生成例句 / 正在整理搭配信息，完成的打勾。
- 错误（spec_6 / strict_5）：红色感叹图标 + 「生成失败」+ 原因 + 「重新生成」按钮（只重试当前标签）。
- 空状态（spec_7）：文档图标 + 「点击上方选项卡查看内容」+ 说明文字。
- 头部菜单（spec_9）：点 pin 切换置顶；右键头部弹出「置顶窗口 / 取消置顶 / 最小化 / 关闭」。

### 7.5 动作
- 喇叭：新增 `ui/tts.rs`，用 SAPI `ISpVoice::Speak`（`windows` crate 开 `Win32_Media_Speech` 特性），异步标志 + 独立 COM 线程，en-US 语音。
- 复制：沿用 `copy_to_clipboard`，提示「已复制」1.5 s 后消失。
- 收藏：沿用 `LearningVault::save_expression`。
- 练一句：沿用 `StartDrillSession`。
- 问 AI：打开 AI 问答面板，主题设为当前表达，并预填欢迎卡片。

### 7.6 代码组织
`ui/companion.rs` 拆成 `ui/companion/{mod.rs, model.rs, layout_header.rs, tab_nuance.rs, tab_alternatives.rs, tab_dialogue.rs, states.rs, input.rs}`，每个文件 < 500 行；`dispatch/mod.rs` 里的三路请求挪到 `dispatch/companion/mod.rs`。

验收：七种状态的预览 PNG 与参考裁图并排对比通过；钉住时打字不覆盖内容（保留现有行为）；所有按钮点击命中区与绘制一致（hitmap 单测）。

---

## P8　AI 深度问答面板（约 2–3 天）

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
- 关思考为默认；「深度」开关打开时才允许思考，思考内容折叠显示。
- 流式重绘 ≤ 30 fps；Markdown 只重新解析最后一个气泡。

代码：`ui/ai_assistant.rs` 拆成 `ui/ai_assistant/{mod.rs, sidebar.rs, topic_bar.rs, messages.rs, composer.rs, input.rs}`。

验收：ai-chat 预览 PNG 与 strict_4 并排通过；新建 / 切换 / 清空会话可用，重启后会话还在；流式期间 CPU 不再整窗满载。

---

## P9　回归、文档、部署（约 0.5 天）

沿用 `docs/plan/ai-current-input-optimization.md` 的 Phase 5 清单：`cargo fmt --check`、`cargo test -p qingjian-core --lib`、`-p qingjian-predict --lib`、`-p qingjian-windows-server`、`cargo check -p qingjian-windows-server`、`git diff --check`、`qingjian-cli --replay` 与 `--ai-bench`。部署前备份 `E:\Qingjian\qingjian-server.exe`，核对 SHA-256；非必要不换 TSF DLL。同步 `docs/notes/crate-notes.md` 与 `CHANGELOG.md`。

---

## 给 Sonnet 的执行规则
1. 按 P0 → P1 → P2 … 顺序；每个 Phase 一个提交，提交信息用 Conventional Commits，说明用中文，不加 AI 署名。
2. 任何改动都不能让本地候选变慢；网络失败必须静默降级。
3. 单文件 ≤ 800 行（目标 500）；新文件要有 `//!` 头；错误用 `thiserror`；依赖用 `cargo add` 并提到 workspace。
4. 不把用户输入、光标前文写进日志；私密输入框不发任何请求。
5. 每个界面 Phase 都要先出预览 PNG，并和 `docs/design/reference/` 并排对比，再接到 Windows 窗口。
6. Windows Server 只能在 Windows 机器上编译和实测；在 Linux 上只跑 core / predict / render 的测试和预览。

## 待用户决定（未回复时按推荐执行）
1. AI 联想形态：**A 整句补全 + 云端词（推荐）** / B 只重排 / C 上屏后联想。
2. 纠错展示：**高置信时直接放第 1 位并带「纠」角标（推荐）** / 只在右侧提示，按 Tab 采用。
3. 模型：打字通道用更快的小模型（Qwen3-8B 关思考，或 Qwen2.5-7B-Instruct），面板继续用 Qwen3-14B 关思考。**推荐：P0 实测后选 P95 最低且准确率达标的那个。**
4. 头像素材：需要 2 张卡通头像（A 男生、B 女生）。
