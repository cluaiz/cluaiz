# Run 36: the official Ollama Python library against the Engine (after the fix)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M, isolated
`CLUAIZ_HOME` on port 8011, `ollama` Python library 0.6.1. Script: `probe.py`; output: `results.txt`.

| Call | Run 33 (before, raw HTTP) | Run 36 (after, the library) |
|---|---|---|
| `client.chat(...)` | `/api/chat` answered in OpenAI's shape | `content='hello'`, `done=True`, `done_reason=stop`, `eval_count=1`, `prompt_eval_count=19` |
| `client.chat(..., stream=True)` | SSE in OpenAI's shape | 26 lines of JSON, counting 1 to 5, final `done=True`, `eval_count=25` |
| `think=True` | `think` ignored | 765 characters in `message.thinking`, answer `2 + 3 = 5.` |
| `format="json"` | - | `{"city": "Paris"}`, parses as JSON |
| `client.generate(..., system=...)` | 404 | `response='yes'`, `done=True` |
| `client.list()` | worked | `['qwen3-0.6b-gguf-Q4_K_M']` |
| `GET /api/version` | 404 | `{"version":"0.1.0"}` |
| `client.embed` with a chat model | 404 | HTTP 400 `Selected model does not declare an embedding task`: the same refusal as `/v1/embeddings` |

## What changed (code)

`api/src/handlers/ollama.rs`: `/api/chat`, `/api/generate`, `/api/embed`, `/api/version`. An Ollama request
becomes the OpenAI request of the chat or embeddings handler (`options`, `think`, `format`, `keep_alive`
in seconds, `images`, `tools`), and the answer goes back in Ollama's shape: one object, or one JSON object
per line when streaming. Unit tests in the file (request, `keep_alive`, generate, final object).

## Not proven

One model, one platform. `num_ctx`, `images`, `tools` and `/api/embed` with an embedding model were not
sent. The Ollama JavaScript library was not run. The `api` test suite (with the new unit tests) was not
run: it rebuilds llama.cpp with CUDA; the library run is the check. `/api/ps` and `/api/show` are still 404.
