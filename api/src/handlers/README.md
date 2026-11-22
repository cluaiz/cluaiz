# Direct HTTP route handlers (`api/src/handlers/`)

![The HTTP routes behind the auth check: chat, models, embeddings and speech, and CEP](../../../docs/assets/api/route-map.png)

This folder holds direct inference, document extraction and compatibility routes. Engine control operations are CEP methods in [`../cep/`](../cep/) (`POST /v1/cep`, `/mcp`, `cep_call`).

```mermaid
graph LR
    Client["OpenAI and Ollama clients"] --> Router["routes.rs"]
    Router --> Chat["chat/: /v1/chat/completions, /api/chat"]
    Router --> Ingest["ingest/: /v1/ingest"]
    Ingest --> Emb["embeddings.rs: shared EmbeddingExecutor"]
    Router --> Emb["embeddings.rs: /v1/embeddings"]
    Router --> Audio["audio.rs: /v1/audio/speech, /v1/audio/transcriptions"]
    Router --> Models["models.rs: /v1/models, /api/tags, /api/show, /api/pull"]
    Router --> Health["health.rs: /health"]
    Router --> CEP["../cep: /v1/cep, /mcp"]
    CEP -->|"methods reuse"| Chat
    CEP -->|"methods reuse"| Audio
    CEP -->|"methods reuse"| Emb
```

| File | Routes | Shared with CEP |
|---|---|---|
| `ollama.rs` | `/api/chat`, `/api/generate`, `/api/embed`, `/api/ps`, `/api/version` | Ollama's request turned into the OpenAI request of `chat/` or `embeddings.rs`, and the answer turned back: one object, or one JSON object per line when streaming (Ollama's default). Ollama's `keep_alive` number is seconds; it is turned into the Engine's minutes. Unit tests in the file; the Ollama Python library 0.6.1 against it in run 36. |
| `chat/` | `/v1/chat/completions` (and `/api/chat` through `ollama.rs`) | The whole chat pipeline. `chat.*`, `memory.*`, `context.*` and `approvals.*` call into `chat/recall.rs`, `chat/authorize.rs`, `chat/context_plan.rs` and `chat/background.rs`. |
| `ingest/` (`mod.rs`, `model.rs`) | `/v1/ingest` | Shared document extraction/source resolution and existing embedding/native model owners; direct final JSON, not a CEP task |
| `embeddings.rs` | `/v1/embeddings` | `EmbeddingExecutor` is reused by `/v1/ingest` with a model |
| `audio.rs` (+ `audio_types.rs`, `audio_tests.rs`) | `/v1/audio/speech`, `/v1/audio/transcriptions` | One dispatch for the standard routes and `audio.run`; `audio.voices` / `audio.languages` use its model selection |
| `models.rs` | `/v1/models`, `/api/tags`, `/api/show`, `/api/pull` | `/api/tags` sends the main file's change time as `modified_at` (run 35). `/api/show` reads the GGUF header CEP `models.inspect` reads (`GgufProber::read_header`, no load) and answers in Ollama's `ShowResponse` shape: `details`, `model_info` (header keys, numbers typed back, long arrays `[]`, `general.name` and `tokenizer.chat_template` left out as Ollama does), `template`, `modified_at`; the parameter size is the tensor element count (Ollama counts the same way, `fs/gguf/metadata.go`). Unknown model: 404 `{"error":"model 'x' not found"}`, Ollama's text. Checked with curl on 2026-11-22 (OLMoE-1B-7B: `parameter_size` `6.9B`; unknown model 404; empty body 400). The registry listing used by `models.list` lives in `cluaiz_models`. `/api/pull` and CEP `models.download` share `pull()` here, which runs `cluaiz_models::fetcher::pull` (the resolver `cluaiz pull` uses). |
| `health.rs` | `/health` | - |

Rules for this folder:
- Ingest stays a direct inference route; new Engine control operations are CEP methods (`../cep/methods/`).
- A function needed by both a route and a method lives in one place, and both call it.

## Tests

23 unit tests in this folder and below (chat 20, `audio_tests.rs` and others 3); route tests in
`api/tests/`. `api` builds both native backends, so none of these tests were run on 2026-11-21 (see `api/README.md`, Tests).

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `/api/show` sends no tensor list (`verbose`) and no `capabilities` | Ollama clients that read capabilities (tools, vision) see none | ggml tensor type names; a mapping from the package capabilities |
| `/api/tags` `parameter_size: Unknown` when the registry has no count | an empty size class in Ollama clients | the GGUF parameter count in the registry |
| OpenAI fields not supported on installed models (`n` > 1, `logprobs`, `logit_bias`): refused with HTTP 400 since 2026-11-22, no longer ignored | clients that need them get an error | pass them to llama.cpp (`chat/README.md`, Pending) |
