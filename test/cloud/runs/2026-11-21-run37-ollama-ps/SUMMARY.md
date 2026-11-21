# Run 37: `/api/ps` with the Ollama Python library

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011, `ollama` 0.6.1. Script: `probe.py`; output: `results.txt`.

| Call | Result |
|---|---|
| `client.ps()` before any chat | `models=[]` |
| `client.ps()` after one chat | one model: `qwen3-0.6b-gguf-Q4_K_M`, `size` 396,705,472 bytes (its file), `size_vram` 1,597,270,011 (the VRAM its plan was given, KV cache and buffers included), `context_length` 16384, `format` gguf, `family` qwen3 |

Code: `api/src/handlers/ollama.rs` (`ps`), from the host's current plan and the model registry.

## Not proven

`size_vram` is the planned budget, not a driver reading. `parameter_size` is `Unknown` for this model
(the registry has no count). `expires_at` and `digest` are not sent.
