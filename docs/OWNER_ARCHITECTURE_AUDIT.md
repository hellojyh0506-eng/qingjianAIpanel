# QINGJIAN WINDOWS ARCHITECTURE AUDIT

## 1. 鏍稿績闂绛斿

**1. 褰撳墠 Qingjian 鏄惁宸茬粡鑳藉瀵规暣鍙ュ仛缈昏瘧锛?*
[CODE-VERIFIED] 鏄殑銆傚湪 `crates/qingjian-predict/src/prompt.rs` 涓紝宸茬粡瀹屾暣瀹炵幇浜?`PredictionKind::Translate`锛屽苟涓旀湁涓撳睘鐨?`TRANSLATE_SYSTEM_PROMPT`銆係erver 鐨?`dispatch/translate/mod.rs` 涔熶細璋冪敤 `engine.request_translation(text)`銆?
**2. 褰撳墠鑳藉姏涓轰粈涔堟湁鏃惰〃鐜颁负绌猴紵**
[CODE-VERIFIED] 鏍规嵁婧愮爜锛屽鏋?`text.is_empty()` 鎴?`engine.prediction_enabled() == false`锛屽垯鐩存帴蹇界暐銆?[INFERRED] 姝ゅ锛岃嫢瀹夸富搴旂敤涓嶆敮鎸?TSF `ITextStoreACP::GetSelection` 璇诲彇閫夊尯鍐呭锛孴SF 浼犲洖鐨?text 浼氫负绌猴紱鎴栬€呯綉缁滆姹傝秴鏃?澶фā鍨嬭繑鍥為潪娉?JSON 鏃讹紝涔熶細琛ㄧ幇涓洪潤榛樺け璐ワ紙鏃犲€欓€夋垨绌虹粨鏋滐級銆?
**3. 鐜版湁浜戣仈鎯充笌鈥滆嚜鐒惰嫳鏂囩炕璇戔€濈殑鍖哄埆鏄粈涔堬紵**
[CODE-VERIFIED] 浜戣仈鎯充娇鐢ㄧ殑鏄?`PredictionKind::Compose`锛屽叾 System Prompt 鏃ㄥ湪鈥滄牴鎹敤鎴风殑鎷奸煶琛ュ叏鎴栫籂姝ｅ綋鍓嶇殑涓枃璇嶅彞鈥濓紱鑰岃嚜鐒惰嫳鏂囩炕璇戜娇鐢ㄧ殑鏄?`PredictionKind::Translate`锛屽叾 System Prompt 鏄庣‘鎸囦护涓衡€滄妸鐢ㄦ埛閫変腑鐨勪腑鏂囧瓧绗︿覆鐩存帴缈昏瘧涓虹洰鏍囪瑷€锛堥粯璁?English锛夆€濄€?
**4. 鏄惁闇€瑕佹柊 PredictionKind锛?*
[CODE-VERIFIED] 涓嶉渶瑕併€傚綋鍓嶇殑 `PredictionKind::Translate` 宸茬粡瓒冲鏀拺浠庘€滆瘝鍙ョ炕璇戔€濆埌鈥滄暣娈电炕璇戔€濈殑闇€姹傘€?
**5. 鏄惁闇€瑕佹柊鐨?TranslationProvider锛?*
[CODE-VERIFIED] 涓嶉渶瑕併€傚綋鍓嶇殑 `Predictor` 宸茬粡鎶借薄浜?HTTP 璇锋眰涓?Prompt 缁勮锛屽苟涓旀敮鎸佸紓姝ヨ繑鍥?`Reply`锛屽彲浠ョ洿鎺ュ鐢ㄣ€?
**6. 鏄惁鍙互瀹屽叏澶嶇敤鐜版湁 qingjian-predict锛?*
[CODE-VERIFIED] 鏄殑銆傚綋鍓嶅熀浜?reqwest 鐨?HTTP 鏍堛€侀噸璇曟満鍒躲€丣SON 瑙ｆ瀽鍣ㄥ強 `PredictionRequest` 鏁版嵁缁撴瀯瀹屽叏婊¤冻鏂拌鍒掔殑 AI 鑻辫瀛︿範鐗规€с€?
**7. 褰撳墠 English Direct 鏄惁宸茬粡瀛樺湪锛?*
[CODE-VERIFIED] 瀛樺湪 `english_mode` 鐘舵€佷綅銆傚畠鍦?`crates/qingjian-core/src/engine/session.rs` 涓畾涔夛紝鐢?TSF 鐨?`mode_state` (Shift / Caps Lock) 瑙﹀彂銆?
**8. 濡傛灉瀛樺湪锛屼负浠€涔堢敤鎴蜂綋楠屼笉鍍?Microsoft English input锛?*
[CODE-VERIFIED] 鍥犱负鍦?`apps/windows/tsf/src/com/service/key_sink.rs` 涓紝`eats_key` 鏂规硶鍦?`modifiers.english_mode` 涓?true 鏃朵緷鐒惰繑鍥?`true`銆傝繖鎰忓懗鐫€ TSF 鎷︽埅浜嗘墍鏈夎嫳鏂囧瓧姣嶅苟鍙戦€佺粰 Server锛孲erver 鐨?Engine 浼氫负鍏剁敓鎴愨€滆嫳鏂囧€欓€夎瘝琛ㄢ€濓紙鏈€澶?18 涓€欓€夛紝`ENGLISH_MODE_CANDIDATES`锛夈€傚畠璧扮殑鏄?IME Composition 娴佺▼锛岃€屼笉鏄師鐢熺殑 Passthrough銆?
**9. 褰撳墠涓枃鍊欓€変笉瓒崇殑鏍瑰洜鏄粈涔堬紵**
[INFERRED ONLY] 鍙兘鐢变簬 `Predictor` 鐨?`CloudWord` 鑾峰彇瓒呮椂鎴栬闃叉姈 (debounce) 鎴柇锛屾垨鑰呮湰鍦扮殑 N-Gram 鍙?`fuzzy` 瑙勫垯鏈兘鍖归厤鍒扮敓鍍昏瘝姹囩粍鍚堛€傚€欓€夌己澶卞彂鐢熷湪鍏蜂綋鍝竴灞備粛鏈‘瀹氾紝闇€瑕佷娇鐢ㄧ湡瀹炲鐜版牱鏈繘琛?profiling銆?
**10. 褰撳墠 selection translation 鐨勭湡瀹炲疄鐜版槸浠€涔堬紵**
[CODE-VERIFIED]
1. 鐢ㄦ埛鎸変笅蹇嵎閿紙濡?`Ctrl+Alt+T`锛夈€?2. TSF 鍖归厤鍒?`preserved::GUID_TRANSLATE`锛屽皢鎸夐敭 Forward 缁?Server銆?3. Server `matches_translate_combo` 鎴愮珛锛屽悜 TSF 鍙戦€?IPC 娑堟伅 `ServerMessage::RequestSelection`銆?4. TSF 鎵ц COM 鎺ュ彛璇诲彇瀹夸富鐨勯€夊尯鏂囨湰涓?`ScreenRect`锛岄€氳繃 `handle_selection` 浼犲洖 Server銆?5. Server 璋冪敤 `engine.request_translation(text)` 鍙戣捣浜戠缈昏瘧銆?
**11. 褰撳墠 Server UI 濡備綍宸ヤ綔锛?*
[CODE-VERIFIED] Server 鏄嫭绔嬬殑 exe銆傚畠鍦?`apps/windows/server/src/ui/` 涓嬪埄鐢?Windows 鐨?`Layered Window` (閫忔槑鎮诞绐? 鎶€鏈洿鎺ョ粯鍒跺€欓€夋 (`Candidate Frame`)銆傛墍鏈?UI 娓叉煋涓?TSF UI 绾跨▼瀹屽叏瑙ｈ€︼紝闃叉姝婚攣銆?
**12. 鍝簺鑳藉姏蹇呴』鐣欏湪 TSF锛?*
[CODE-VERIFIED] 
- `ITfTextInputProcessor` / `ITfKeyEventSink` 鐨?COM 娉ㄥ唽銆?- `eats_key` 鎸夐敭鎷︽埅鍒ゅ畾銆?- Host Application 鐨?Text Store 瀹炵幇 ITextStoreACP锛決ingjian TSF 閫氳繃 TSF/COM 缂栬緫浼氳瘽璁块棶瀹夸富鎻愪緵鐨勬枃鏈帴鍙ｃ€?- IPC 瀹㈡埛绔紝璐熻矗涓?Server 楂橀€熼€氫俊銆?
**13. 鍝簺鑳藉姏缁濆涓嶈兘杩涘叆 TSF锛?*
[CODE-VERIFIED] 
- HTTP 鏍堜笌澶фā鍨嬭姹傦紙浼氬鑷村涓诲簲鐢ㄧ嚎绋嬬洿鎺ュ崱姝伙級銆?- SQLite / FSRS锛堝鍔?DLL 浣撶Н涓旀槗瀵艰嚧澶氳繘绋嬮攣搴撳啿绐侊級銆?- Candidate UI 鐨勬覆鏌撳眰锛堜細瀵艰嚧璺ㄨ繘绋嬬劍鐐规姠澶烘閿侊級銆?- 瀛︿範璁板繂涓庨敊棰樻湰绛夊簽澶т笟鍔￠€昏緫銆?
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

## 3. Recommended Modifications (PHASE 2 鍙婁箣鍚?

1. **English Direct Mode (PHASE 2)**
   - 鏂囦欢锛歚apps/windows/tsf/src/com/service/key_sink.rs`
   - 鍔ㄤ綔锛氫慨鏀?`eats_key` 鍜?`eats_without_server`銆傚綋妫€娴嬪埌 `english_candidates = false` 涓斿浜?`english_mode` 鏃讹紝TSF 搴旂洿鎺ヨ繑鍥?`FALSE`锛屽皢鎸夐敭 Passthrough 缁欏涓伙紝涓斾笉寤虹珛 Composition銆?
2. **Passive English Learning (PHASE 4)**
   - 鏂囦欢锛歚crates/qingjian-core/src/engine/mod.rs` & `apps/windows/server/src/dispatch/translate/mod.rs`
   - 鍔ㄤ綔锛氬鍔犱竴涓潪闃诲鐨勫悗鍙伴槦鍒椼€傚綋鐢ㄦ埛杈撳叆涓枃鍒拌揪鍙ユ湯鏍囩偣锛屾垨鏆傚仠 (Pause) 瓒呰繃闃堝€兼椂锛岃Е鍙戜竴涓悗鍙扮殑 `PredictionKind::Translate`锛屽苟鍦ㄧ粨鏋滆繑鍥炴椂杩藉姞鍒?Candidate 鍒楄〃灏鹃儴锛堟垨鐗规畩瑙嗚浣嶏級锛屼笉骞叉壈榛樿鏁板瓧閫夊彇銆?
3. **Vault / Mistake Bank / FSRS (PHASE 10-11)**
   - 鏂囦欢锛氭柊寤?`crates/qingjian-learning` 鐨勬ā鍧楁垨鍦?`server` 灞傚鍔?SQLite wrapper銆?   - 鍔ㄤ綔锛氱粷涓嶆斁鍏?TSF銆傛墍鏈?`Save` 鍔ㄤ綔鐢?UI 灞傛垨蹇嵎閿淳鍙?IPC 缁?Server 寮傛鎵ц銆?
## 4. Test Harness Plan

- `tests/direct_mode.rs`: 鏂█褰撻厤缃?`english_candidates = false` 鏃讹紝`key_sink` 鐨勬寜閿悶鍣€昏緫绗﹀悎棰勬湡銆?- `tests/translation.rs`: 鎵╁睍瀵?`prompt.rs` 鐨勬祴璇曪紝鏂█甯︽湁 B2/C1 tagging 涓?Markdown Diff 鐨?Prompt 鏄惁琚畨鍏ㄧ粍瑁咃紝涓旀棤 Prompt Injection銆?
## 5. 鍗遍櫓鏂囦欢鍒楄〃 (DO NOT TOUCH)
- `apps/windows/tsf/src/com/thread_mgr.rs` (鏋佸害鑴嗗急鐨?COM 鐢熷懡鍛ㄦ湡)
- `crates/qingjian-platform/src/` (搴曞眰 IPC 绠￠亾閫昏緫锛岀ǔ瀹氾紝鏃犻渶鏀瑰姩)

## 6. PHASE 0 & 1 Correction Updates

- **Backup Status**: [CODE-VERIFIED] A complete safety backup was created in `C:\Users\Admin\.gemini\antigravity\scratch\qingjian_backup_v2`, covering the source tree, installed binary, user data, local app data, and TSF registry configuration. A `manifest.csv` with SHA256 hashes was generated.
- **Installed Binary Mapping**: [CODE-VERIFIED] The currently installed `qingjian_tsf-0.1.4.dll` corresponds exactly to git tag `v0.1.4` (commit: `f7abaefcb1a3aeaca5c01692941a64a7b1f43eb5`). The source tree `HEAD` is ahead at commit `c08ae57cb88b6a4a46f4a5e9c1d6d11c5e69222e`.
- **Restore Rehearsal Status**: [CODE-VERIFIED] A temporary restore rehearsal script was successfully executed, verifying the restoration of files and the execution of the restore steps without overwriting the actual installation.
- **English Direct Behavior Flow**: [CODE-VERIFIED] Analyzed and documented in `docs/ENGLISH_DIRECT_MODE_BEHAVIOR.md`. The lack of native typing feel is caused by TSF continually intercepting alphabetic keys (as `eats_key` returns `true` for `english_mode`) and sending them to the Server, which then injects the characters back via IME composition rather than passing them through natively.
- **UI Focus Verification**: [CODE-VERIFIED] The Candidate Frame and Status windows are created with `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, effectively preventing any focus stealing from the host application.
