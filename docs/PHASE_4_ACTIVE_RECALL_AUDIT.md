# PHASE 4 — ACTIVE RECALL 架构与能力审计报告

本文档基于真实代码审查，为 Phase 4（Active Recall 主动回忆）提供能力审计、边界定义与最小切片实现方案。

---

## 一、Phase 3 产生与可获得的状态与数据审计

| 数据项 | 当前代码状态 | 存在位置与实现机制 | 证据等级 |
|---|---|---|---|
| **User Exposure (用户曝光)** | **真实存在** | `crates/qingjian-core/src/engine/learning/mod.rs` 中 `record_exposures()` 收集当前候选页的各译词 `(language, word)`，在候选上屏或确认时调用 `self.vocabulary.record_exposure()` 计入曝光。 | `[CODE-VERIFIED]` |
| **Translation Result (翻译结果)** | **真实存在** | `crates/qingjian-core/src/candidate/translation/mod.rs` 的 `Translation` 结构体，以及 `apps/windows/server/src/dispatch/composed/mod.rs` 中的 `self.passive_translation: Option<String>`。 | `[CODE-VERIFIED]` |
| **English Sense (英文义项)** | **真实存在** | `crates/qingjian-core/src/candidate/translation/sense.rs` 的 `Sense { text, part_of_speech, reading, fresh }`。本地词典 `translator` 和被动翻译均挂载在候选词中。 | `[CODE-VERIFIED]` |
| **Source Phrase (中文源词/短语)** | **真实存在** | `crates/qingjian-core/src/engine/commit/mod.rs` 中的 `candidate.text`（例如 `"开发"`）。在调用 `commit_with()` 时，候选的中文源词与对应的英文译词（`candidate.translation`）同时处于同一上下文。 | `[CODE-VERIFIED]` |
| **Timestamp (时间戳)** | **真实存在** | `crates/qingjian-learning/src/vocabulary_book.rs` 中使用 `jiff::civil::Date` 记录 `first` 与 `last` 访问日期；`input_log.rs` 中记录精确到毫秒/时区的 ISO 8601 时间戳字符串 `t`。 | `[CODE-VERIFIED]` |
| **Learning-Language 状态** | **真实存在** | `crates/qingjian-core/src/candidate/language.rs` 与 `Config.general.learning_language`（支持 `en`, `ja`, `es`, `off`）。配置为 `"off"` 时，`learning_language()` 为 `None`，所有词义注入与学习逻辑完全旁路。 | `[CODE-VERIFIED]` |
| **Privacy 状态** | **真实存在** | `crates/qingjian-core/src/engine/mod.rs` 的 `self.private: bool`；在密码框、安全输入、无痕窗口下被激活，此时清空候选暂存、禁止记录曝光、禁止云端预测、禁止写入日志。 | `[CODE-VERIFIED]` |

---

## 二、当前 Learning / 持久化数据模型审计

| 存储项 | 当前数据模型支持现状 | 差异与缺口 | 证据等级 |
|---|---|---|---|
| **Expression (目标英文表达)** | **已支持** | `user-vocab.tsv`（由 `VocabularyBook` 管理）存储 `word: String`。 | `[CODE-VERIFIED]` |
| **Meaning (中文提示/源词义)** | **不支持** | 当前 `VocabularyBook` 仅按 `(language, word)` 键值聚合，未持久化记录对应的中文源词（如 `"开发"`）。`user-vocab.tsv` 格式为 `语言\t译词\t看到轮次\t上屏次数\t用过次数\t首见\t末见`。 | `[CODE-VERIFIED]` |
| **Exposure Count / Last Seen** | **已支持** | `VocabularyBook.entries` 记录了 `seen: u32`、`committed: u32`、`used: u32`、`first: Date`、`last: Date`。 | `[CODE-VERIFIED]` |
| **Recall State (回忆状态)** | **不支持** | 当前没有回忆状态（如 `Pending`, `Passed`, `Failed`, `Revealed`）及回忆正确率统计字段。 | `[CODE-VERIFIED]` |
| **Review History (复习历史)** | **不支持** | 既有代码中不存在复习历史流水或主动回忆事件结构。 | `[CODE-VERIFIED]` |
| **SQLite 数据库** | **不存在** | 代码库中目前没有任何 SQLite 或 `rusqlite` 依赖。所有现有持久化均为原子写入的 TSV 与 JSONL 文件。 | `[CODE-VERIFIED]` |

---

## 三、当前 Active Recall 基础设施承载能力审计

| 模块组件 | 当前代码现状 | 处置策略 | 证据等级 |
|---|---|---|---|
| **Learning Item (回忆条目)** | **不存在** | 需新增轻量级 `RecallItem { prompt: String, target: String, language: Language, status: RecallStatus, ... }`。 | `[CODE-VERIFIED]` |
| **Scheduler (复习调度器)** | **不存在** | Phase 4 仅做最小确定性队列：按先入先出或最近曝光顺序调度待复习条目，禁止引入复杂 FSRS 调度算法。 | `[CODE-VERIFIED]` |
| **Score / State (评测与状态)** | **不存在** | 需新增确定性匹配评测：严格区分 `Correct`（主动回忆正确）、`Incorrect`（错误）、`Revealed`（Tab 查看答案）、`Passive`（被动曝光）。 | `[CODE-VERIFIED]` |
| **Review Event (复习事件记录)** | **不存在** | 需在 `input-log.jsonl`（`InputLogEntry`）增加 `Recall` 事件变体，并在 TSV 中记录持久化累计结果。 | `[CODE-VERIFIED]` |
| **Candidate UI / Learning UI** | **已具备完整支撑** | Windows Server UI 的 `Frame` 原生支持 `candidates`、`preedit` 与 `notice: Option<String>`。`apps/windows/server/src/ui/candidates/view.rs` 中 `draw_notice` 可直接渲染提示与题目，无需新增任何原生窗口句柄。 | `[CODE-VERIFIED]` |
| **IPC 消息** | **具备扩展点** | 协议 `ClientMessage` / `ServerMessage` 已具备帧通信管道，可复用现有按键转发或新增最小触发指令。 | `[CODE-VERIFIED]` |

---

## 四、能力清单：真实存在 vs 必须新增

### 1. 真实存在的能力 `[CODE-VERIFIED]`
- 中文源词 + 英文译词在 `commit_with()` 时的关联上下文。
- 候选窗口的 `notice` 行渲染能力（已在 `view.rs` 中实现 `draw_notice`）。
- 类似 `Translation` 评审模式的交互架构（`apps/windows/server/src/dispatch/translate/mod.rs`）：捕获状态机、按键拦截、不破坏宿主编辑。
- `read_text_lossy` 与 `write_atomic_str` 原子文件持久化机制。
- 保护正常的 `Typing > Learning > AI` 优先级，普通候选词选择零干扰。

### 2. Phase 4 必须新增的最小能力 `[PROPOSAL]`
- **`crates/qingjian-learning/src/recall_book.rs`**：
  提供 `RecallBook` 持久化管理（落盘到 `%APPDATA%\Qingjian\user-recall.tsv`），管理 `(prompt, target)` 映射、曝光累加、回忆成功/失败/查看答案计数。
- **`crates/qingjian-core/src/engine/recall/mod.rs`**：
  在 Core 建立 `RecallTracker` 抽象，在 `commit_with()` 中当候选词存在英文译文且非私密模式时，自动生成/更新 Recall 条目。
- **Deterministic Matcher (确定性判分器)**：
  对用户输入进行 `trimmed` 与 `to_lowercase` 归一化判定，精准判定是否与预期目标表达匹配。
- **Server Router Recall State Machine**：
  在 `apps/windows/server/src/dispatch/recall/` 增加 `RecallSession`，用户通过快捷键（如 `Ctrl + Alt + R`）或命令主动唤起：
  - 显示题目：`notice: "【主动回忆】中文: 开发 (输入英文, Enter提交, Tab看答案, Esc退出)"`
  - 用户输入字母直接在 `preedit` 中更新预览
  - 按 `Enter` 执行确定性判分，给出结果并持久化
  - 按 `Tab` 显式查看答案（记录为 `Revealed`，绝不混淆为成功 recall）
  - 按 `Esc` 放弃并退出
  - **全程绝不向宿主 Text Store 写入任何字符**，完全隔离于原生输入。
