# English Direct Mode Behavior

## Data Flow
Key -> TSF -> Server -> Router -> Engine -> Server IPC Response -> TSF -> Document Insert

### 1. key_sink `eats_key`
[CODE-VERIFIED] When `english_mode` is true, `eats_key` intercepts all alphabetic keys. The TSF consumes the key and forwards it to the Server via IPC.

### 2. Disconnected Server
[CODE-VERIFIED] If the Server is disconnected, `eats_without_server` evaluates to `false` for `english_mode = true` or `caps = true`. The keys are natively passed through to the Host Application.

### 3. Server Router & `english_candidates=false`
[CODE-VERIFIED] Inside the Server (`apps/windows/server/src/dispatch/key/input.rs`), the `Router::apply_key` determines `english_candidates`. If the global configuration is false (or the app is in the off list), `english_candidates` evaluates to `false`.
The router calls `apply_english(c, false, event)`.

### 4. `apply_english` & Passthrough
[CODE-VERIFIED] In `apply_english` with `candidates = false`:
If the key is a letter, the Server does **not** return `Effect::Passthrough`. Instead, it returns `Effect::Changed(Some(c.to_string()))`.
When this reaches the TSF, the `consumed` flag is `true`. The TSF then calls `update_document` which inserts the character via COM `ITextStoreACP::InsertTextAtSelection` / composition interfaces. 
**This is why it doesn't feel like native Microsoft English input:** It uses IME text injection instead of native keyboard hardware events (passthrough).

### 5. apply punctuation
[CODE-VERIFIED] If the character is punctuation in English mode (with `candidates=false`), `apply_english` calls `apply_punctuation`. If no full-width translation is needed (e.g., standard half-width English punctuation), it returns `Effect::Passthrough`.
The TSF then natively passes the punctuation key to the application.
*(Notice the inconsistency: Letters are IME-injected, but punctuations are passed through).*

### 6. composition boundary
[CODE-VERIFIED] If a user is composing Chinese (e.g. `ni`) and switches to English or types an English character that resolves to `english_candidates=false`, `Router::apply_key` captures `self.composing()` and extracts the raw pinyin via `engine.take_raw()`. This raw text is flushed and prepended to the new English character commit string, effectively ending the composition and committing both.

### 7. Shift/Caps
[CODE-VERIFIED]
- **Caps Lock**: Sets `caps = true`, which implies `english = true` and `english_candidates = false`. Letters are IME-injected just like `english_candidates=false`.
- **Shift (Chinese Mode)**: If not composing, Shift+Letter is evaluated as `!eats_key` and natively passed through to the app. If `shift_letter_compose` is true, it enters the composition buffer.

### 8. Mixed Chinese-English (inside composition)
[CODE-VERIFIED] In Chinese mode, typing an uppercase letter with Shift (if `shift_letter_compose` is false) natively passes the letter through to the app. If composing, uppercase letters trigger `take_raw()` to flush the composition. 

### 9. Number / Punctuation Handling
[CODE-VERIFIED] In Chinese mode outside composition, numbers and punctuation are evaluated for full-width conversion. If no conversion is needed, they return `Effect::Passthrough` and are natively passed to the Host App. Inside composition, numbers act as candidate selectors (e.g., `gpt6` -> `6` selects the 6th candidate if it exists, otherwise it's buffered or committed).

### 10. Space, Backspace, Enter, Esc
[CODE-VERIFIED] When `english_candidates = false` (English Direct Mode) and not composing Chinese, `eats_key` evaluates Space, Backspace, Enter, and Esc as non-letters and non-punctuation/digits. Since `composing = false`, `eats_key` returns `false`. These keys are natively passed through to the Host Application without Server intervention.

### 11. Shortcuts (Ctrl / Alt / Win)
[CODE-VERIFIED] When `modifiers.has_command_key()` is true and not composing, `eats_key` immediately returns `false`. All shortcuts (e.g., Ctrl+C) are natively passed through to the Host Application.
