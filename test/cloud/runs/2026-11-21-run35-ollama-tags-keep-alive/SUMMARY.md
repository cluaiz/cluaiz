# Run 35: `/api/tags` dates and `keep_alive` durations (after the fix)

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011. Script: `probe.py`; output: `results.txt`; the unload times are the server's own log lines.

| Request | Run 33 (before) | Run 35 (after) |
|---|---|---|
| `GET /api/tags` | `modified_at` always `2026-10-08T00:00:00Z` | `2026-11-21T01:53:27Z`, the model file's change time |
| `keep_alive: "5m"` | HTTP 422 | HTTP 200; log: unload in 5 minutes |
| `keep_alive: "30s"` | - | HTTP 200; log: unload in 1 minute (seconds round up) |
| `keep_alive: "1h"` | - | HTTP 200; log: unload in 60 minutes |
| `keep_alive: "-1"`, `0`, `5` | - | HTTP 200; no unload, unload after the answer, unload in 5 minutes |
| `keep_alive: "soon"` | - | HTTP 422 |

## What changed (code)

- `api/src/utils/mod.rs`: `keep_alive_minutes` reads a number (minutes, as before) or a string with `s`,
  `m` or `h`; used by chat, embeddings, speech and transcription requests. Unit test
  `keep_alive_takes_minutes_or_ollama_durations`, run on its own with `rustc --test`: 1 passed.
- `api/src/handlers/models.rs`: `/api/tags` sends the main file's change time.

## Not proven

`parameter_size` still comes from the registry (`Unknown` for this model). The `api` test suite was not
run (it rebuilds llama.cpp with CUDA); speech and transcription requests with a string `keep_alive`
were not sent.
