# Cluaiz Engine API Reference

Reference for the engine's HTTP routes (market-standard routes plus CEP at `/v1/cep`), model selection, media payloads and response formats. Use the endpoint pages below for accepted fields, examples and current limitations.

---

## 🌐 Base URL & Endpoints

| Protocol | Default Local Endpoint | Description |
|:---|:---|:---|
| **HTTP REST** | `http://127.0.0.1:8000` | Default engine address; use your configured host/port |
| **SSE** | `http://127.0.0.1:8000/v1/chat/completions` | Chat token streaming over HTTP; audio routes have their own event schema |
| **CEP** | `http://127.0.0.1:8000/v1/cep` | Every Cluaiz-specific method (JSON-RPC 2.0); see [cep/README.md](./cep/README.md) |

---

## 🔒 Authentication & Token Security

Cluaiz utilizes an industry-standard (OpenAI / vLLM compatible) **Bearer Token Authentication Architecture**:

```http
Authorization: Bearer <TOKEN>
```

### Supported Tokens
1. **Configured API Key (`sk-cluaiz-...`):** Managed in Developer Hub under Settings -> API Keys (saved in `permission.json`).
2. **Local Session Token:** Automatically generated on boot and stored in `~/.cluaiz/session.token` with restricted (`0600`) POSIX permissions.

### Route Enforcement Matrix

| Route | Auth Toggle OFF | Auth Toggle ON |
|---|---|---|
| `GET /health` | Public | Public |
| Everything else: `/v1/cep`, `/mcp`, `/v1/chat/completions`, `/v1/embeddings`, `/v1/audio/*`, `/v1/models`, `/api/*` | Open | **Bearer Token Required (401)** |

The current middleware bypasses Bearer checks when `api_auth.required` is false. Examples use a real session or configured token; placeholder API keys are not valid authentication. Filesystem paths are still checked separately (canonical path, the caller's roots); a Bearer token does not replace that.

## Two kinds of routes

- **Market-standard routes**, for existing clients:
  - OpenAI: `/v1/chat/completions`, `/v1/embeddings`, `/v1/audio/speech`, `/v1/audio/transcriptions`, `/v1/models`
  - Ollama: `/api/chat`, `/api/tags`, `/api/pull`
  - `/health`
- **Everything Cluaiz-specific is CEP**, one JSON-RPC 2.0 endpoint: `POST /v1/cep`.
  - Files, system, settings, models, tools, chat memory, notes, context, audio tasks, ingest, approvals.
  - Start with [cep/README.md](./cep/README.md). Every method is in [cep/methods.md](./cep/methods.md).
  - The old REST routes are listed there with their replacement.

## Model selection and media routes

`models.list` returns the installed models. Use an exact `id`, and inspect `format_type`, `supported_tasks`, `metadata` and `experimental`. Task names can use hyphens or underscores. Registry declarations identify candidates; runtime assets still determine compatibility.

PowerShell discovery on the engine's own computer (the configured token/home may differ):

```powershell
$base = 'http://127.0.0.1:8000'
$tokenPath = Join-Path $env:USERPROFILE '.cluaiz/session.token'
$token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
$headers = @{ Authorization = "Bearer $token" }
$call = @{ jsonrpc = '2.0'; id = 1; method = 'models.list'; params = @{} } | ConvertTo-Json
$catalog = (Invoke-RestMethod "$base/v1/cep" -Method Post -Headers $headers -ContentType 'application/json' -Body $call).result
$catalog | Select-Object id, category, format_type, supported_tasks, experimental
# Save the Body Payload as request.json after selecting your installed model.
$body = Get-Content -LiteralPath './request.json' -Raw
Invoke-RestMethod "$base/v1/embeddings" -Method Post -Headers $headers `
    -ContentType 'application/json' -Body $body
```

| Goal | Route | Which model to choose |
|:---|:---|:---|
| Chat / document questions | `POST /v1/chat/completions` | `file_url`: native document text; PDF page images for a selected image-capable package with projector |
| Image / video questions | Same chat route, `image_url` / `video_url` | Chat/vision model with matching projector; video audio also requires audio capability |
| Audio questions | Same chat route, `audio_url` / `input_audio` | Audio-capable chat model, not TTS |
| Make speech | `POST /v1/audio/speech` | Compatible ONNX TTS or GGUF speech package |
| Transcribe a recording | `POST /v1/audio/transcriptions` | Compatible ONNX STT package |
| Text vectors | `POST /v1/embeddings` | Compatible ONNX/GGUF text embedding package, including supported EmbeddingGemma-2 text projection |
| Image vectors | Same embeddings route, `{type:"image",source:...}` input | ONNX image embedding graph; GGUF multimodal embeddings are not implemented |
| Extract document text/chunks | CEP `ingest.run`, omit model | Native PDF/OCR/DOCX/DOC/text extraction |
| Document or image model processing | CEP `ingest.run`, select model | Text embedding model for native chunks; ONNX image graph for images/PDF pages; generative GGUF vision model for text |

`ingest.run` extracts chunks and can run a selected embedding or vision model. It is a task: poll `tasks.result`. `/v1/embeddings` accepts prepared text or image inputs directly. Your application stores, indexes and searches the returned vectors; these routes do not create a persistent RAG database. Retention for chat, speech, STT and direct embeddings is in **minutes** (`0` unloads, positive expires, `-1` retains). Ingest has no per-request retention field; its selected model uses the existing owner's cached lifecycle.

---

## 📦 Dedicated API Documentation Modules

### 1. CEP (Cluaiz Engine Protocol)
* [**`POST /v1/cep`**](./cep/README.md): the envelope, errors, tasks, approvals, MCP (`/mcp`, `cluaiz mcp-server`), the C library (`cep_call` / `cep_free`), and the old-route table.
* [**Method reference**](./cep/methods.md): every method with params, scope and example (generated; [openrpc.json](./cep/openrpc.json)).

### 2. Inference & Multimodal Execution
* [**`POST /v1/chat/completions`**](./inference/chat_completions.md): conversational dialogue, multimodal input, streaming SSE, reasoning tokens. Control a running stream with CEP `chat.cancel` / `chat.skip_reasoning`.
* [**Chat memory**](./inference/chat_history.md): `memory.*` (event log, Chat Map, recall, export/import) and `notes.*`.
* [**Context runtime**](./inference/context_runtime.md): `context.*` and `config` section `context`.
* [**`POST /v1/embeddings`**](./inference/embeddings.md): vector generation with GGUF/ONNX routing.
* [**`POST /v1/audio/speech`**](./inference/audio_speech.md): text-to-speech; voices with CEP `audio.voices`, speech tasks with `audio.run`.
* [**`POST /v1/audio/transcriptions`**](./inference/audio_transcriptions.md): speech-to-text; languages with CEP `audio.languages`.
* [**`ingest.run`**](./inference/ingest.md): document extraction and optional model processing, as a CEP task.
* [**`POST /v1/rerank`**](./inference/rerank.md): cross-encoder document re-ranking.

### 3. Model Management
* [**`GET /v1/models` & `GET /api/tags`**](./models/list_models.md): list installed and active models (CEP `models.list`, `models.load`, `models.inspect`, `models.remove` manage them).
* [**`POST /api/pull`**](./models/pull_model.md): download weights.

### 4. System
* [**`GET /health`**](./system/health.md): engine health.
* [**`system.plan`**](./system/plan.md): the execution plan for a model against live hardware, without loading it.
* Everything else (`system.info`, `system.live`, `system.ps`, `system.exec`, `storage.*`, `config.*`, `tokens.*`, `approvals.*`, `fs.*`, `workspace.*`, `tools.*`): [cep/methods.md](./cep/methods.md).

---

## ⚡ OpenAI Client Quickstart (Python)

Cluaiz Engine provides drop-in OpenAI Python and TypeScript SDK compatibility:

```python
from openai import OpenAI

# Connect to local Cluaiz Engine
client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="sk-cluaiz-e46c11e877b20afb49c4df0fcfa44351"
)

response = client.chat.completions.create(
    model="llama_3.2_instruct-3b",
    messages=[
        {"role": "system", "content": "You are a helpful AI assistant."},
        {"role": "user", "content": "Explain prefix caching in 2 sentences."}
    ],
    temperature=0.7,
    max_tokens=256
)

print(response.choices[0].message.content)
```

---

## 🚨 Standard Error Format

A missing or wrong token (with authentication on) is HTTP 401:

```json
{
  "error": "Unauthorized",
  "message": "API Authentication is required. Provide a valid Bearer token in the Authorization header."
}
```

The standard routes answer errors in their own format (OpenAI style `{"error": {"message", "type"}}`). CEP answers JSON-RPC errors with HTTP 200: see [cep/README.md](./cep/README.md#errors).
