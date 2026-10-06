# QINGJIAN WINDOWS ARCHITECTURE AUDIT

## 1. 核心问题答复

**1. 当前 Qingjian 是否已经能够对整句做翻译？**
[VERIFIED] 是的。在 `crates/qingjian-predict/src/prompt.rs` 中，已经完整实现了 `PredictionKind::Translate`，并且有专属的 `TRANSLATE_SYSTEM_PROMPT`。Server 的 `dispatch/translate/mod.rs` 也会调用 `engine.request_translation(text)`。

**2. 当前能力为什么有时表现为空？**
[VERIFIED] 根据源码，如果 `text.is_empty()` 或 `engine.prediction_enabled() == false`，则直接忽略。
[INFERRED] 此外，若宿主应用不支持 TSF `ITextStoreACP::GetSelection` 读取选区内容，TSF 传回的 text 会为空；或者网络请求超时/大模型返回非法 JSON 时，也会表现为静默失败（无候选或空结果）。

**3. 现有云联想与“自然英文翻译”的区别是什么？**
[VERIFIED] 云联想使用的是 `PredictionKind::Compose`，其 System Prompt 旨在“根据用户的拼音补全或纠正当前的中文词句”；而自然英文翻译使用的是 `PredictionKind::Translate`，其 System Prompt 明确指令为“把用户选中的中文字符串直接翻译为目标语言（默认 English）”。

**4. 是否需要新 PredictionKind？**
[VERIFIED] 不需要。当前的 `PredictionKind::Translate` 已经足够支撑从“词句翻译”到“整段翻译”的需求。

**5. 是否需要新的 TranslationProvider？**
[VERIFIED] 不需要。当前的 `Predictor` 已经抽象了 HTTP 请求与 Prompt 组装，并且支持异步返回 `Reply`，可以直接复用。

**6. 是否可以完全复用现有 qingjian-predict？**
[VERIFIED] 是的。当前基于 reqwest 的 HTTP 栈、重试机制、JSON 解析器及 `PredictionRequest` 数据结构完全满足新规划的 AI 英语学习特性。

**7. 当前 English Direct 是否已经存在？**
[VERIFIED] 存在 `english_mode` 状态位。它在 `crates/qingjian-core/src/engine/session.rs` 中定义，由 TSF 的 `mode_state` (Shift / Caps Lock) 触发。

**8. 如果存在，为什么用户体验不像 Microsoft English input？**
[VERIFIED] 因为在 `apps/windows/tsf/src/com/service/key_sink.rs` 中，`eats_key` 方法在 `modifiers.english_mode` 为 true 时依然返回 `true`。这意味着 TSF 拦截了所有英文字母并发送给 Server，Server 的 Engine 会为其生成“英文候选词表”（最多 18 个候选，`ENGLISH_MODE_CANDIDATES`）。它走的是 IME Composition 流程，而不是原生的 Passthrough。

**9. 当前中文候选不足的根因是什么？**
[INFERRED ONLY] 可能由于 `Predictor` 的 `CloudWord` 获取超时或被防抖 (debounce) 截断，或者本地的 N-Gram 及 `fuzzy` 规则未能匹配到生僻词汇组合。候选缺失发生在具体哪一层仍未确定，需要使用真实复现样本进行 profiling。

**10. 当前 selection translation 的真实实现是什么？**
[VERIFIED]
1. 用户按下快捷键（如 `Ctrl+Alt+T`）。
2. TSF 匹配到 `preserved::GUID_TRANSLATE`，将按键 Forward 给 Server。
3. Server `matches_translate_combo` 成立，向 TSF 发送 IPC 消息 `ServerMessage::RequestSelection`。
4. TSF 执行 COM 接口读取宿主的选区文本与 `ScreenRect`，通过 `handle_selection` 传回 Server。
5. Server 调用 `engine.request_translation(text)` 发起云端翻译。

**11. 当前 Server UI 如何工作？**
[VERIFIED] Server 是独立的 exe。它在 `apps/windows/server/src/ui/` 下利用 Windows 的 `Layered Window` (透明悬浮窗) 技术直接绘制候选框 (`Candidate Frame`)。所有 UI 渲染与 TSF UI 线程完全解耦，防止死锁。

**12. 哪些能力必须留在 TSF？**
[VERIFIED] 
- `ITfTextInputProcessor` / `ITfKeyEventSink` 的 COM 注册。
- `eats_key` 按键拦截判定。
- Host Application 的 Text Store 实现 ITextStoreACP；Qingjian TSF 通过 TSF/COM 编辑会话访问宿主提供的文本接口。
- IPC 客户端，负责与 Server 高速通信。

**13. 哪些能力绝对不能进入 TSF？**
[VERIFIED] 
- HTTP 栈与大模型请求（会导致宿主应用线程直接卡死）。
- SQLite / FSRS（增加 DLL 体积且易导致多进程锁库冲突）。
- Candidate UI 的渲染层（会导致跨进程焦点抢夺死锁）。
- 学习记忆与错题本等庞大业务逻辑。

---

## 2. Architecture & Data Flow Diagram

```mermaid
flowchart TD
    subgraph Host Application (WeChat/Chrome)
        TSF[Qingjian TSF DLL]
        TextStore[ITextStoreACP]
    end

    subgraph Qingjian Server Process
        IPC_Server[IPC Dispatcher]
        Engine[qingjian-core Engine]
        UI[Layered Window UI]
        Predictor[qingjian-predict HTTP]
        LocalDict[qingjian-dictionary]
        Learner[qingjian-learning SQLite/FSRS]
    end

    Cloud[LLM Cloud API]

    Host Application -- "Keys (e.g. 'h', 'e', 'l')" --> TSF
    TSF -- "IPC Message (KeyEvent)" --> IPC_Server
    IPC_Server --> Engine
    Engine --> Predictor
    Predictor -- "HTTP Async" --> Cloud
    Cloud -- "JSON Translation" --> Predictor
    Predictor --> Engine
    Engine -- "Candidate Frame" --> UI
    Engine -- "Update Vocab" --> Learner
    TSF <--> TextStore
```

## 3. Recommended Modifications (PHASE 2 及之后)

1. **English Direct Mode (PHASE 2)**
   - 文件：`apps/windows/tsf/src/com/service/key_sink.rs`
   - 动作：修改 `eats_key` 和 `eats_without_server`。当检测到 `english_candidates = false` 且处于 `english_mode` 时，TSF 应直接返回 `FALSE`，将按键 Passthrough 给宿主，且不建立 Composition。

2. **Passive English Learning (PHASE 4)**
   - 文件：`crates/qingjian-core/src/engine/mod.rs` & `apps/windows/server/src/dispatch/translate/mod.rs`
   - 动作：增加一个非阻塞的后台队列。当用户输入中文到达句末标点，或暂停 (Pause) 超过阈值时，触发一个后台的 `PredictionKind::Translate`，并在结果返回时追加到 Candidate 列表尾部（或特殊视觉位），不干扰默认数字选取。

3. **Vault / Mistake Bank / FSRS (PHASE 10-11)**
   - 文件：新建 `crates/qingjian-learning` 的模块或在 `server` 层增加 SQLite wrapper。
   - 动作：绝不放入 TSF。所有 `Save` 动作由 UI 层或快捷键派发 IPC 给 Server 异步执行。

## 4. Test Harness Plan

- `tests/direct_mode.rs`: 断言当配置 `english_candidates = false` 时，`key_sink` 的按键吞噬逻辑符合预期。
- `tests/translation.rs`: 扩展对 `prompt.rs` 的测试，断言带有 B2/C1 tagging 与 Markdown Diff 的 Prompt 是否被安全组装，且无 Prompt Injection。

## 5. 危险文件列表 (DO NOT TOUCH)
- `apps/windows/tsf/src/com/thread_mgr.rs` (极度脆弱的 COM 生命周期)
- `crates/qingjian-platform/src/` (底层 IPC 管道逻辑，稳定，无需改动)

## 6. PHASE 0 & 1 Correction Updates

- **Backup Status**: [VERIFIED] A complete safety backup was created in `C:\Users\Admin\.gemini\antigravity\scratch\qingjian_backup_v2`, covering the source tree, installed binary, user data, local app data, and TSF registry configuration. A `manifest.csv` with SHA256 hashes was generated.
- **Installed Binary Mapping**: [VERIFIED] The currently installed `qingjian_tsf-0.1.4.dll` corresponds exactly to git tag `v0.1.4` (commit: `f7abaefcb1a3aeaca5c01692941a64a7b1f43eb5`). The source tree `HEAD` is ahead at commit `c08ae57cb88b6a4a46f4a5e9c1d6d11c5e69222e`.
- **Restore Rehearsal Status**: [VERIFIED] A temporary restore rehearsal script was successfully executed, verifying the restoration of files and the execution of the restore steps without overwriting the actual installation.
- **English Direct Behavior Flow**: [VERIFIED] Analyzed and documented in `docs/ENGLISH_DIRECT_MODE_BEHAVIOR.md`. The lack of native typing feel is caused by TSF continually intercepting alphabetic keys (as `eats_key` returns `true` for `english_mode`) and sending them to the Server, which then injects the characters back via IME composition rather than passing them through natively.
- **UI Focus Verification**: [VERIFIED] The Candidate Frame and Status windows are created with `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, effectively preventing any focus stealing from the host application.
