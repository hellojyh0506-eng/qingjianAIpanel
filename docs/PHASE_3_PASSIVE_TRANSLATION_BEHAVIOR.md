# PHASE 3 — PASSIVE TRANSLATION BEHAVIOR SPECIFICATION & VERIFICATION

**Document Version:** 1.0  
**Baseline Git HEAD:** `5faf626`  
**Current Branch:** `owner/v0.1.4-english-learning`  
**Core Principles:** `Typing > Learning > AI` · `Three-second Principle` · `Appropriateness > Sophistication`

---

## 1. 概述与核心交互模式 (Overview)

被动翻译（Passive Translation）旨在实现：
> “用户正常输入中文时，青简提供英文学习提示，但绝不打断正常中文输入。”

翻译作为**旁路学习信息**，绝对不会变成常规中文候选项，也绝对不会在用户按 `Space` 或 `Enter` 时自动上屏插入正文。

---

## 2. 用户看到什么与视觉布局 (User Presentation)

### 2.1 候选窗口视觉呈现
- **中文候选区域**：
  正常显示本地算法与语言模型计算出的中文候选词或整句。
  例如用户输入 `kaifa`：
  ```text
  [1] 开发   v. develop · n. development
  [2] 开发者
  [3] 开放
  [4] 开饭
  [5] 开始
  ```
  或者输入 `xianqian`（词库有词但随包释义表无本地项时，云端被动翻译返回）：
  ```text
  [1] 先前   previously
  [2] 线圈
  [3] 显现
  ```

### 2.2 视觉与交互不变性
1. **[CODE-VERIFIED]** **英文提示不占据独立候选序号**：
   英文仅作为候选 1 的伴随标注（`Candidate.translation` -> `Row.annotation`），不会出现“候选 1 是先前，候选 2 是 previously”的情况。
2. **[CODE-VERIFIED]** **不干扰数字选词**：
   用户按数字键 `1..9` 选词时，选择的是对应的中文候选（`Candidate.text`）。
3. **[CODE-VERIFIED]** **自绘窗口焦点安全**：
   候选窗口带有 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`，渲染与更新绝不抢夺宿主应用光标焦点。

---

## 3. 中文输入如何继续工作 (Input Continuity)

- **[CODE-VERIFIED]** **Key Path 零阻塞**：
  所有键盘按键处理（`OnTestKeyDown`, `OnKeyDown`, `ClientMessage::Key`）均为同步、内存级 O(1) 操作。
  翻译请求绝不在键盘路径上同步等待网络或模型返回，而是在后台异步派发。
- **[CODE-VERIFIED]** **翻译失败完全静默**：
  若网络不可用、API 超时、返回空文本或格式错误，中文候选照常工作，候选框仅不显示英文批注，中文输入流 100% 顺畅。

---

## 4. 触发策略与优先级分流 (Trigger & Routing)

被动翻译采用“本地极速优先，云端旁路兜底”的分级机制：

```text
用户输入拼音 ──> 生成中文候选
                     │
                     ▼
             候选 1 有本地随包释义？
             ┌───────┴───────┐
             │ 是            │ 否
             ▼               ▼
        0ms 同步挂载    learning_language != "off"
        本地离线释义    且非私密输入？
                             │
                      ┌──────┴──────┐
                      │ 是          │ 否
                      ▼             ▼
                 异步发起       不触发翻译
              PredictionKind::
                 Translate
```

1. **本地离线层（0ms）**：
   优先通过 `Engine::translate_word` 查验随包及个人离线释义词典（`glossary-en.qj` 44,924 条）。若命中，同步赋予 `candidate.translation`，无网络开销与延迟。
2. **云端被动层（异步）**：
   若候选 1 为长词或短语且本地词库无释义，在 `learning_language != "off"` 且未处于密码/私密输入时，向 `Predictor` 派发 `PredictionKind::Translate`。
   若本地已命中，则正常维持云端整句联想 `PredictionKind::Compose`。

---

## 5. 为什么绝不会自动上屏 (Zero Accidental Insertion)

- **[CODE-VERIFIED] & [INTEGRATION-TEST]** **Space / Enter 提交流程**：
  - 用户按 `Space` 或 `Enter` 时，Server 调用 `commit_highlighted()` -> `commit_index(highlight)`。
  - `commit_index` 取出对应槽位的 `Candidate`，调用 `Engine::commit(&candidate)`。
  - `Engine::commit` 的实现仅将 `candidate.text`（中文）作为提交文本返回，`candidate.translation` 仅作为学习统计用途记录，**绝不上屏到应用程序**。
- **[CODE-VERIFIED] & [INTEGRATION-TEST]** **主动译词提交（Ctrl+1）**：
  - 只有当用户显式按下配置的译词修饰键组合（Windows 默认为 `Ctrl+1`）时，Server 调用 `commit_translation_on_page(1, 0)` -> `Engine::commit_translation`，此时才会主动上屏英文译文。

---

## 6. Stale Result 过期丢弃机制 (Stale Rejection)

必须防止上一轮输入的翻译响应延迟到达，错误挂载到新一轮输入候选上。

- **[CODE-VERIFIED] & [INTEGRATION-TEST]** **Sequence Identity 校验**：
  1. `Engine::request_translation` 发出时自增 `prediction_sequence`。
  2. 用户继续敲键、退格、Esc 或提交时，`recompose()` 立即将 `self.passive_translation = None` 清空，并触发 `cancel_prediction()`（`prediction_sequence += 1`）。
  3. `poll_prediction()` 只有在收到结果的 `sequence == self.prediction_sequence` 时才采纳。任何陈旧的在飞响应在到达时因序号不匹配被直接静默丢弃。

---

## 7. 密码框与私密上下文 (Password & Privacy Context)

- **[CODE-VERIFIED] & [UNIT-TEST]** **私密隔离策略**：
  - 当宿主应用处于密码框、PIN 输入或安全输入时，TSF DLL 向 Server 发送 `ClientMessage::Privacy { private: true }`。
  - Server 设置 `self.engine.set_private(true)`。
  - `recompose()` 中检查 `!self.is_private()`，私密输入下绝对不发起被动翻译请求。
  - `Engine::request_translation` 同样设置硬件级守卫：`if self.private { return None; }`，彻底防止敏感文本外泄。

---

## 8. 学习开关（learning_language = "off"）行为

- **[CODE-VERIFIED] & [UNIT-TEST]** **全局学习关闭**：
  - 当配置 `[general] learning_language = "off"` 时：
    1. Server 装配时不加载释义表，注入 `NoTranslator`。
    2. `RouterConfig.learning_language_off` 置为 `true`。
    3. `recompose()` 判断 `learning_on` 为 `false`，绝不触发被动翻译。
    4. 候选框仅呈现纯净中文候选，无任何英文字段与学习标注。

---

## 9. English Direct Mode 零回归保证

- **[CODE-VERIFIED] & [REAL-EMPIRICAL]** **直通路径独立性**：
  - Phase 2B 的 Native English Direct Mode 驻留在 TSF DLL 的 `key_sink.rs`（`eats_key`）。
  - 当 `english_direct = true` 且处于英文模式时，TSF `eats_key` 直接返回 `false`，键盘消息被宿主 Windows 原生处理，**根本不进入 Server，不进入 Router，不触发任何 candidate 或 translation 逻辑**。
  - Phase 3 所有修改均位于 `qingjian-core` 与 `qingjian-windows-server` 的中文组句分发层，完全不触碰 TSF DLL 键盘直通路径。
  - 实机测试验证：`gpt6`、`deepseek-r1`、`C++`、`I'm`、`123`、`hello world` 字符顺序与原生放行 100% 保持一致。

---

## 10. 选区翻译（Ctrl+Alt+T）零回归保证

- **[CODE-VERIFIED] & [UNIT-TEST]** **状态机互斥与隔离**：
  - 选区翻译使用独立的 `self.translation: Option<Translation>` 模态。
  - 被动翻译使用独立的 `self.passive_translation: Option<String>` 状态。
  - 在 `poll_prediction()` 中，若 `self.translation.is_some()`，优先填充选区翻译结果并提前返回，二者生命周期完全隔离，互不干扰。

---

## 11. 证据等级与验证矩阵 (Evidence Matrix)

| 测试项 / 场景 | 证据标签 | 验证位置与方法 | 结果 |
| :--- | :---: | :--- | :---: |
| **本地词库被动释义展示** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_local_glossary_passive_translation_space_commits_chinese_only` | **PASS** |
| **Space 仅上屏中文候选** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_local_glossary_passive_translation_space_commits_chinese_only` | **PASS** |
| **Enter 仅上屏生键或拼音** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_local_glossary_enter_commits_raw_pinyin_never_english` | **PASS** |
| **云端被动翻译异步挂载** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_cloud_passive_translation_when_no_local_gloss_displayed_and_space_commits_chinese` | **PASS** |
| **Ctrl+1 主动上屏被动翻译** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_ctrl_1_commits_passive_translation` | **PASS** |
| **Stale 过期响应自动丢弃** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_stale_response_discarded_on_new_typing` | **PASS** |
| **learning_language=off 禁用**| `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_learning_language_off_disables_all_passive_translation` | **PASS** |
| **私密/密码模式禁用翻译** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_privacy_mode_disables_passive_translation` | **PASS** |
| **退格与 Esc 立即清理** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_backspace_and_esc_clears_passive_translation` | **PASS** |
| **English Direct 配置下发无回归** | `[UNIT-TEST]` | `tests/engine_loop/passive_translation.rs`: `test_english_direct_mode_no_regression` | **PASS** |
| **真实 Windows 命名管道端到端** | `[INTEGRATION-TEST]` | `tests/pipe.rs`: `named_pipe_passive_translation_and_space_commits_chinese_only` | **PASS** |
| **真实 Windows Host 键盘直通** | `[REAL-EMPIRICAL]` | `apps/windows/tsf/tests/real_host_keyboard_empirical.rs`: `test_real_windows_host_keyboard_validation_end_to_end` | **PASS** |
| **TSF COM 生命周期与投递** | `[REAL-EMPIRICAL]` | `apps/windows/tsf/tests/tsf_com_empirical.rs`: `test_real_tsf_com_lifecycle_and_all_scenarios` | **PASS** |

---

## 12. 边界限制与下一阶段展望 (Uncovered & Deferred)

- **本阶段严格不包含 Phase 4+ 能力**：
  - 不包含 Active Recall（主动回想与隐藏对比）；
  - 不包含 FSRS 遗忘曲线调度算法；
  - 不包含 SQLite 词汇库与 Mistake Bank；
  - 不包含 TTS 语音发音；
  - 不包含 CEFR 难度标签扩展；
- **后续优化空间**：
  - 可增加云端翻译结果的本地会话级 LRU 缓存命中优化（目前依靠 `CloudPredictor` 现有的 64 条缓存）。
