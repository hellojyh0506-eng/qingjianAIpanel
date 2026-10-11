# Long-Pinyin Intent Latency Follow-up — 2026-10-11

## Evidence from live logs

The previous request order serialized three API calls for the screenshot input: candidate rerank around 2,164 ms, passive translation around 676 ms, and Intent around 1,118 ms. The corrected suggestion therefore arrived only after roughly 4 seconds. This is too late for a fast typist.

## Latency changes

- The Core detects a suspicious 7–64 letter full-pinyin input and immediately checks the already available local candidates for a valid full reading within one edit. Chinese word candidates are preferred over sentence-fragment candidates; the best local match is placed first and surfaced in the existing preedit-right hint before any model response.
- For a suspicious long-pinyin input, Server no longer starts the normal Rerank -> Translate chain. It starts the dedicated Intent request directly. If the model later returns a different locally validated candidate, it may refine the hint and candidate order only while the user has not navigated away from the current composition.
- Worker Intent debounce is 50 ms. A normal Rerank for a full-pinyin string of at least 7 letters waits 280 ms, creating a small window for a trailing accidental key to turn that request into Intent. Debounce is trailing-edge: each new request resets the window; in-flight noncritical requests remain interruptible.
- If Intent yields no accepted candidate, the fast local hint stays in place and the prior passive translation path resumes. Normal input continues to use the existing Rerank / passive-translation behavior.

## Validation

- Core: 333/333 unit tests passed.
- Predict: 33/33 unit tests passed, including debounce-policy coverage.
- Windows Server: all test targets passed, including 89 input-loop cases and the immediate-local-hint + Intent + Tab acceptance integration test.
- `cargo check -p qingjian-windows-server`, `git diff --check`, and `cargo build --release -p qingjian-windows-server` passed.
- Release SHA-256 from this build: `CAEDC396FE43F3173A363949ADA3E72C3E2B635B40D57DCDA01B19758E0CA105`.

## Deployment state

This note is a record of the build before deployment. The live binary is still the prior stable version until a separately verified backup-and-deploy step completes. No TSF DLL changes are part of this work.
