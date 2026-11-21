# Run 33: what an Ollama client gets from the Engine (probe)

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011. Ten requests in Ollama's format, written while documenting the move from Ollama. The script is
`probe.py`; its printed output is `results.txt` (bodies cut to 600 characters).

| Request | Result | What Ollama answers |
|---|---|---|
| `GET /api/tags` | HTTP 200, Ollama's shape; `modified_at` is always `2026-10-08T00:00:00Z`, `parameter_size` `Unknown` | the file's date and size class |
| `POST /api/chat`, `stream: false` | HTTP 200 in **OpenAI's** shape (`choices[0].message`), not Ollama's (`message`, `done`) | Ollama's shape |
| `POST /api/chat` without `stream` | HTTP 200, one JSON reply (no stream) | Ollama streams by default |
| `POST /api/chat`, `stream: true` | `text/event-stream` with OpenAI chunks (`data: {...}`) | `application/x-ndjson`, one JSON object per line |
| `think: false` | ignored: the reply has `reasoning_content` | no thinking |
| `keep_alive: "5m"` | HTTP 422: `expected i32` (the Engine takes minutes as a number) | a duration string or seconds |
| `/api/generate`, `/api/ps`, `/api/show`, `/api/embed`, `/api/version` | HTTP 404 | each answers |

## Cause (code)

- `/api/chat` is routed to the same handler as `/v1/chat/completions` (`api/src/routes.rs:38-39`); there
  is no Ollama request or response mapping.
- `/api/tags` writes a fixed `modified_at` (`api/src/handlers/models.rs:96`).
- Only `/api/tags`, `/api/chat` and `/api/pull` exist under `/api/` (`api/src/routes.rs`).

## What this means

An OpenAI client works with either chat route. The Ollama Python and JavaScript libraries and other
clients that parse Ollama's reply shape do not work against `/api/chat` today; `/api/tags` lists the
installed models in a shape they can read.

## Not proven

One model, one platform. `/api/pull` was not sent (it downloads). No Ollama library was run; the
failure for Ollama clients follows from the reply shape, not from a library test.
