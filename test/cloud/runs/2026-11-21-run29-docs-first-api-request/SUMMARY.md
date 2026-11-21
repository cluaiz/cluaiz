# Run 29: the first API request page, recorded (2026-11-21)

Windows 11, Engine 0.1.0 (`target/release/cluaiz.exe`), an isolated `CLUAIZ_HOME` on port 8011 (never the
owner's port-8000 server or `~/.cluaiz`). Script: `first-api.sh`; output: `output.txt` (keys replaced by
`<key>`). Source of every output in `docs/get-started/first-api-request.md`.

| Step | Result |
|---|---|
| `cluaiz pull qwen3:0.6b` | not a catalogue ID: "No matching Cluaiz model ID"; the page uses the full repository ID |
| `cluaiz pull unsloth/Qwen3-0.6B-GGUF:Q4_K_M` | installed as `qwen3-0.6b-gguf-Q4_K_M`, 0.40 GB |
| `GET /health` | 200, `"status":"alive"` |
| chat, `think_mode: off`, `max_tokens: 20` | `"content":"**Paris**"`, `finish_reason: stop` |
| chat, streamed with `include_usage` | usage chunk first, then text deltas, then `[DONE]` |
| wrong model | 400 `model_not_found` |
| `cluaiz token create` while the server runs | the running server still answers without a key (200) |
| after restarting the server | 401 without the key, 200 with it, `/health` 200 without |

Found: `cluaiz token create` changes `permission.json` only; a running server keeps the setting it started
with. CEP `tokens.create` updates the running server but does not set `api_auth.required`. Documented in
`docs/engine/permission.md`, `first-api-request.md` and `troubleshooting.md`; the code is unchanged (auth
behaviour is the owner's design).
