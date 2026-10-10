# Intent Candidate-Choice Correction — 2026-10-10

## Why the change was needed

The live screenshot for `kai'fa'zhe's` showed that repairing the pinyin string alone was insufficient: local candidates such as “开发这” and “开发者” can share the same reading. Converting repaired pinyin through the default local ranking could choose a different homophone from the user's intended word.

## Behavior now

- Intent replies contain both `corrected_pinyin` and `candidate_text`.
- `candidate_text` must be copied verbatim from the request's local candidate list. Predict rejects invented candidates and rejects candidates whose Core-provided dictionary reading does not exactly equal `corrected_pinyin`.
- Core independently checks bounded edit distance, complete pinyin segmentation, membership in the current local query, and exact candidate syllable reading. A failed or stale result is discarded.
- The selected local text is shown in the existing preedit-right hint. The original typed pinyin remains unchanged; Tab is the acceptance action.
- The Intent system prompt now explicitly resolves same-reading homophones by considering candidate meaning and context, preferring a common standalone term over an incomplete fragment when no cursor-before context contradicts that interpretation.

## Validation

- `cargo test -p qingjian-core --lib`: 333 passed.
- `cargo test -p qingjian-predict --lib`: 32 passed.
- `cargo test -p qingjian-windows-server`: all test targets passed, including the 89-case input-loop suite and the Intent-to-Tab end-to-end test.
- `cargo check -p qingjian-windows-server`, `git diff --check`, and `cargo build --release -p qingjian-windows-server`: passed.
- Code commit: `95d2009 fix(predict): let intent correction select local homophones`.

## Deployment

- Live Windows Server PID: `24540`.
- Live SHA-256: `CC01C2C16D51010EE9ACAA1F5AE66B4F775408491FA0D065AA80F3395405EE32`.
- Desktop verification copy was checked against the same hash.
- Previous live binary backup: `E:\Qingjian\qingjian-server.exe.bak_intent_candidate_choice_20261010_203915`.
- TSF DLL was not replaced.

## Remaining acceptance

The updated live-model behavior has not yet been confirmed with a new real request. Mock-backed tests verify the parser, local-candidate gate, UI hint, and Tab acceptance path, but do not prove that the configured model will consistently select the intended homophone. A real input check should verify `kai'fa'zhe's` suggests “开发者” and that ordinary correct long pinyin is not spuriously corrected.
