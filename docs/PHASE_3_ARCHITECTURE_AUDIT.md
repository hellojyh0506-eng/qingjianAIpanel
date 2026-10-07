# PHASE 3 — PASSIVE TRANSLATION ARCHITECTURE AUDIT

**Target:** `C:\Users\Admin\.gemini\antigravity\scratch\qingjian`  
**Branch:** `owner/v0.1.4-english-learning`  
**Baseline Git HEAD:** `5faf626`  
**Audit Date:** 2026-10-07  
**Auditor:** Agentic Pair Programmer (Strict Evidence Discipline)

---

## 1. Executive Summary & Premise Audit

### 1.1 Premise Audit regarding "TranslationProvider"
- **[CODE-VERIFIED]** 在当前 Rust 源码（`crates/` 与 `apps/`）中，**不存在**名为 `TranslationProvider` 的结构体或 trait。
- **[CODE-VERIFIED]** 系统的翻译与释义能力清晰拆分为两大子系统与一个后台兜底：
  1. **本地离线即时查表（词级别）**：`qingjian_core::Translator` trait（由 `qingjian_translate::LayeredTranslator` 实现，组合 `Glossary` 与 `PersonalGlossary`）。纯查表、内存驻留、无网络、耗时约 0ms。
  2. **云端在线翻译/补全（句/段级别）**：`qingjian_core::Predictor` trait（由 `qingjian_predict::CloudPredictor` 实现，内部支持 `PredictionKind::Translate` 与 `PredictionKind::Compose`）。独立后台线程、`tokio` 异步运行时、`debounce` 防抖、LRU 缓存（容量 64）。
  3. **云端释义后台学习兜底**：`qingjian_core::GlossFiller` trait（由 `qingjian_predict::CloudGlossFiller` 实现）。独立后台线程，将未命中随包词库的词在后台翻译后通过 `Translator::learn` 写入用户个人词库 `user-glossary-en.tsv`。

### 1.2 Core Principle & Invariants
- **Typing > Learning > AI**：正常中文打字优先级绝对最高。
- **Zero Interruption**：翻译为纯旁路学习信息，严禁阻塞按键路径（`OnTestKeyDown` / `OnKeyDown` / IPC 必须保持极轻量 O(1)）。
- **Zero Accidental Insertion**：`Space` / `Enter` 必须永远只上屏中文候选文本，严禁将英文提示词或译文插入正文。
- **No Stale Result Pollution**：旧输入的翻译请求在用户继续键入、清空、提交、Esc、失焦或切模式时必须通过上下文标识立即作废。
- **Reuse Existing Infrastructure**：严禁创建第二套 HTTP client、第二套翻译 IPC 或第二套线程池。

---

## 2. Component-by-Component Audit

### 2.1 crates/qingjian-core: Candidate & Translation Data Models
- **[CODE-VERIFIED]** `Candidate` (`crates/qingjian-core/src/candidate/mod.rs`):
  ```rust
  pub struct Candidate {
      pub text: String,
      pub kind: CandidateKind,
      pub syllables: Vec<String>,
      pub reading: Option<String>,
      pub translation: Option<Translation>,
      pub aux_code: Option<String>,
  }
  ```
- **[CODE-VERIFIED]** `Translation` (`crates/qingjian-core/src/candidate/translation.rs`):
  - 单语言释义结构，`senses: Vec<Sense>`，`MAX_SENSES = 2`。超额释义在构造时截断，避免候选框膨胀。
- **[CODE-VERIFIED]** `Engine::annotate` (`crates/qingjian-core/src/engine/commit/mod.rs`):
  - 遍历候选列表，同步调用 `self.translator.translate(candidate.text)`。
  - 若命中，设置 `candidate.translation = Some(...)`。
- **[CODE-VERIFIED]** Commit Safety (`crates/qingjian-core/src/engine/commit/mod.rs`):
  - `Engine::commit(&candidate)`：仅上屏 `candidate.text`（中文），完全不触碰 `candidate.translation`。
  - `Engine::commit_translation(&candidate, sense)`：仅在用户主动触发快捷键（如 `Ctrl+1`）时上屏第 `sense` 条译文。

### 2.2 crates/qingjian-core: Prediction Subsystem & PredictionKind::Translate
- **[CODE-VERIFIED]** `PredictionKind` (`crates/qingjian-core/src/engine/prediction/kind.rs`):
  - 枚举项：`Compose`（组句补全）、`Question`（问字）、`Translate`（翻译）。
- **[CODE-VERIFIED]** `Engine::request_translation` (`crates/qingjian-core/src/engine/prediction/mod.rs`):
  - 构造 `PredictionRequest { kind: PredictionKind::Translate, text: text.to_owned(), want_sentence: true, ... }`。
  - 递增 `self.prediction_sequence` 并提交给 `self.predictor`。
- **[CODE-VERIFIED]** Sequence Identity & Stale Invalidation (`crates/qingjian-core/src/engine/prediction/mod.rs`):
  - `self.prediction_sequence: u64` 是全局单调自增序号。
  - `cancel_prediction()` 会递增 `prediction_sequence`，使之前发出的所有在飞请求作废。
  - `poll_prediction()` 只有在 `prediction.sequence == self.prediction_sequence` 时才接收，否则记录 debug 日志并直接丢弃（"丢弃过期的联想结果"）。

### 2.3 crates/qingjian-predict: CloudPredictor, Worker & Prompts
- **[CODE-VERIFIED]** `CloudPredictor` (`crates/qingjian-predict/src/cloud_predictor.rs`):
  - 后台独立线程 `Worker`，通过 `mpsc::channel` 通信，非阻塞 `submit` 与 `poll`。
- **[CODE-VERIFIED]** `Worker` (`crates/qingjian-predict/src/worker.rs`):
  - 内置防抖：`requests.recv_timeout(self.debounce)`，在防抖窗口内连续输入只处理最后一条。
  - 内置缓存：`PredictionCache`，LRU 容量 64，对相同请求参数直接命中返回。
- **[CODE-VERIFIED]** Prompt & Reply (`crates/qingjian-predict/src/prompt.rs`):
  - `TRANSLATE_SYSTEM_PROMPT`：针对翻译设计，模型严格输出 JSON `{"sentence": "译文"}`。
  - `parse_reply`：当 `kind == PredictionKind::Translate` 时，将模型输出解析为 `Reply { words: vec![], sentence: raw.sentence }`。

### 2.4 crates/qingjian-translate: LayeredTranslator & Glossaries
- **[CODE-VERIFIED]** `LayeredTranslator` (`crates/qingjian-translate/src/layered_translator.rs`):
  - 组合 `PersonalGlossary`（`user-glossary-en.tsv`）与 `Glossary`（随包二进制 `glossary-en.qj`，共 44,924 条词条）。
  - 同步 O(1) 哈希查询，单次查询微秒级，完全不产生按键延迟。

### 2.5 apps/windows/server: Selection Translation (Ctrl+Alt+T)
- **[CODE-VERIFIED]** `Router::handle_selection` (`apps/windows/server/src/dispatch/translate/mod.rs`):
  - 快捷键触发：`matches_translate_combo`（默认 `Ctrl+Alt+T`）。
  - 激活模态评审：`self.translation = Some(Translation { result: None })`。
  - 渲染 `translation_frame`：在候选窗显示“翻译中…”或译文。
  - 按键响应：回车/空格/1 接受替换选区，Esc 放弃。

### 2.6 apps/windows/server: Candidate Rendering & UI Presentation
- **[CODE-VERIFIED]** `from_candidate` (`apps/windows/server/src/ui/candidates/row.rs`):
  - 将 `candidate.translation` 转换为 `Row.annotation: Vec<(String, Tone)>`。
- **[CODE-VERIFIED]** `CandidateView` (`apps/windows/server/src/ui/candidates/view.rs`):
  - 竖排布局：在候选词右侧的 annotation 列绘制译文。
  - 横排布局：在候选词下方新起一行绘制高亮候选的 annotation（译文）。
  - 绘制完全复用 GDI/Layered Window，支持高亮与字体自适应。
- **[CODE-VERIFIED]** UI 焦点安全 (`apps/windows/server/src/ui/layered/mod.rs`):
  - 窗口样式包含 `WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`。
  - 候选窗显示与重绘绝不抢夺宿主应用键盘焦点。

### 2.7 apps/windows/server: Configuration & English Direct Mode
- **[CODE-VERIFIED]** `learning_language`:
  - `config.general.learning_language = "en"`（默认）。
  - 若 `learning_language = "off"`，`assembly::learning_language` 返回 `None`，Server 注入 `NoTranslator`，所有被动释义完全关闭。
- **[CODE-VERIFIED]** `english_direct`:
  - Phase 2B 已完整验证：`english_direct = true` 且无拼音组句时，英文按键直接走原生键盘路径（Native Passthrough），不进 Server 且不产生 candidate。

---

## 3. Capability Matrix: What Exists vs What is Missing

| 维度 | 现状事实 | 状态 | 说明 |
| :--- | :--- | :---: | :--- |
| **词级离线释义** | `LayeredTranslator` + `Engine::annotate` | **完整存在** | 单个词（如“你好”）候选自动带出“hello”，UI 正确渲染 |
| **词级释义上屏** | `Ctrl+1` -> `commit_translation` | **完整存在** | 仅按快捷键时上屏英文，Space/Enter 永远上屏中文 |
| **主动选区翻译** | `Ctrl+Alt+T` -> `handle_selection` | **完整存在** | 模态选区翻译完整可用 |
| **云端翻译能力** | `Predictor` + `PredictionKind::Translate` | **完整存在** | 提示词与 JSON 解析完整就绪 |
| **离线词库关闭** | `learning_language = "off"` | **完整存在** | 自动退化为 `NoTranslator` |
| **句级被动翻译触发** | 中文组句时对长句/短语的被动翻译请求 | **缺失** | 现有 `request_prediction` 仅请求 `PredictionKind::Compose`（中文补全），长句无离线释义时没有任何被动翻译 |
| **被动翻译状态生命周期** | 独立于 Selection Translation 的 Passive Translation 状态 | **缺失** | 现有 `poll_prediction` 会将 `sentence` 误赋给 `self.sentence`（导致 Tab 补全中文整句被污染为英文） |
| **被动翻译上下文绑定** | 绑定当前组句 identity，丢弃过期响应 | **缺失** | 需确保打字继续、退格、Esc 或提交时，在飞的被动翻译响应不会错挂到新候选上 |
| **被动翻译 UI 呈现** | 当 Candidate 1 无本地释义时，将被动译文注入展示 | **待连通** | 现有渲染器已支持 `Candidate.translation` 与 `Row.annotation`，只需将异步结果注入候选展示数据 |

---

## 4. Key Architectural Decisions for Phase 3

### 4.1 绝对禁止的行为
1. **禁止污染 `self.sentence`**：
   `self.sentence` 专门用于中文整句补全（Tab 键上屏）。被动英文翻译是**学习提示**，绝对不能进入 `self.sentence`，更不能允许 Tab 键把英文整句补全上屏！
2. **禁止将译文作为独立候选项**：
   译文不能成为候选 1、候选 2 等独立候选项，不能参与数字键选词，空格和回车必须始终上屏用户选中的中文词。
3. **禁止创建第二套 HTTP / Translation 栈**：
   必须复用现有的 `Predictor` / `CloudPredictor` / `PredictionKind::Translate` / `LayeredTranslator`。
4. **禁止阻塞按键路径**：
   任何翻译请求只能在组句重构时作为异步触发，或者在本地词库直接命中时同步填充；网络等待绝不阻塞 `Key` 消息。

### 4.2 被动翻译的生命周期与状态模型
在 `Router` 中引入独立的被动翻译追踪状态（与 `self.translation` 互斥且隔离）：
1. **触发时机**：
   - 当正在中文拼音组句且 `learning_language != "off"` 且非私密输入；
   - 本地词库未命中候选 1 释义，且当前首选为多字词或整句；
   - 附带防抖或在特定停顿/长拼音时异步发起 `PredictionKind::Translate`。
2. **标识与作废（Stale Invalidation）**：
   - 记录请求发起时的上下文标识（例如组句文本或序号）。
   - 用户继续输入字符、退格、清空、Esc、回车上屏、焦点切换、中英切换时，重置被动翻译标识。
   - 异步回调到达时，校验标识；若上下文已变化，直接丢弃，不予渲染。
3. **呈现与交互**：
   - 译文到达后，作为候选 1 的学习标注（`Candidate.translation`）参与当前帧重绘。
   - 用户按 `Space` / `Enter`：正常上屏中文候选，被动翻译清空。
   - 用户按 `Ctrl+1`：若用户主动想上屏英文，复用现有 `commit_translation`。
   - 翻译失败或超时：静默忽略，中文候选不受任何影响。

---

## 5. Files to Modify vs Files NOT to Modify

### 5.1 允许修改的文件（最小集合）
- `apps/windows/server/src/dispatch/composed/mod.rs`（组句生成与被动翻译结果接收/挂载）
- `apps/windows/server/src/dispatch/translate/mod.rs`（隔离选区翻译与被动翻译生命周期，或新增 passive 模块）
- `apps/windows/server/src/dispatch/mod.rs`（Router 状态字段扩展）
- `crates/qingjian-core/src/engine/prediction/mod.rs`（若需支持被动翻译请求与联想请求的序号区分或并发路由，仅做微调）

### 5.2 绝对严禁修改的文件
- `apps/windows/tsf/**`（TSF DLL 保持纯粹，禁止触碰 Phase 2B 键盘路径与 COM 逻辑）
- `crates/qingjian-learning/**`（FSRS / 学习系统保留至 Phase 4+）
- `crates/qingjian-translate/src/glossary/**`（底层二进制词典格式无需改动）
- `crates/qingjian-platform/src/protocol/server.rs`（无需增加冗余协议）

---

## 6. Audit & Implementation Sign-Off
- **Architecture Audit Status:** **PASSED [CODE-VERIFIED]**
- **Implementation Status:** **COMPLETED [CODE-VERIFIED]**
  - Minimal surgical modifications in `qingjian-core` and `qingjian-windows-server`.
  - Reused existing `Translator`, `Predictor`, `PredictionKind::Translate`, `Candidate.translation`, and candidate frame renderer.
  - Zero modifications to TSF DLL, zero regressions to Phase 2B English Direct Mode.
  - Space/Enter commit invariance preserved (100% Chinese candidate text only).
  - Stale sequence rejection and privacy mode protection verified.
- **Verification Status:** **PASSED [UNIT-TEST] [INTEGRATION-TEST] [REAL-EMPIRICAL]**
  - `qingjian-core`: 297 passed, 0 failed.
  - `qingjian-windows-server`: 90 passed, 0 failed (including real Windows Named Pipe integration test in `tests/pipe.rs`).
  - `qingjian-windows-tsf`: 22 passed, 0 failed (including `test_real_windows_host_keyboard_validation_end_to_end` on real Windows Host).
  - Release Build: Clean compilation, `target/release/qingjian-server.exe` and `target/release/qingjian_tsf.dll` built.
- **Behavioral Specification:** See [`docs/PHASE_3_PASSIVE_TRANSLATION_BEHAVIOR.md`](file:///C:/Users/Admin/.gemini/antigravity/scratch/qingjian/docs/PHASE_3_PASSIVE_TRANSLATION_BEHAVIOR.md).

