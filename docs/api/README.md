# Cluaiz Engine REST API Reference

Reference for the engine's HTTP routes, model selection, media payloads and response formats. Use the endpoint pages below for accepted fields, examples and current limitations.

---

## 🌐 Base URL & Endpoints

| Protocol | Default Local Endpoint | Description |
|:---|:---|:---|
| **HTTP REST** | `http://127.0.0.1:8000` | Default engine address; use your configured host/port |
| **SSE** | `http://127.0.0.1:8000/v1/chat/completions` | Chat token streaming over HTTP; audio routes have their own event schema |
| **API Explorer** | App API workspace | Ships endpoint definitions from `public/data` and Markdown copies from `public/docs/api` |

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

| Endpoint Category | Examples | Auth Toggle OFF (Local Dev) | Auth Toggle ON (Secured) |
|---|---|---|---|
| **Public Probes** | `GET /health`, `GET /info` | Public (200 OK) | Public (200 OK) |
| **General Inference** | `POST /v1/chat/completions`, `GET /v1/models` | Open for prototyping (200 OK) | **Bearer Token Required (401)** |
| **Sensitive Filesystem** | `POST /v1/fs/read`, `/v1/fs/write`, `/v1/fs/delete` | Auth middleware bypassed; filesystem policy still applies | **Bearer Token Required (401)** |
| **Host Terminal Exec** | `POST /v1/system/cmd` | Auth middleware bypassed; command policy still applies | **Bearer Token Required (401)** |

The current middleware bypasses Bearer checks when `api_auth.required` is false. Examples use a real session/configured token; placeholder API keys are not valid authentication. Source-path authorization is a separate check and is not replaced by a Bearer token or `x-caller: user`.

## Model selection and media routes

`GET /v1/models/installed` returns `models`/`installed` arrays and an `installed_models` map. Use an exact `id`; inspect `format_type`, `supported_tasks`, `metadata` and `experimental`. Task names can use hyphens or underscores. Registry declarations identify candidates; runtime assets still determine compatibility.

PowerShell discovery on the engine's own computer (the configured token/home may differ):

```powershell
$base = 'http://127.0.0.1:8000'
$tokenPath = Join-Path $env:USERPROFILE '.cluaiz/session.token'
$token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
$headers = @{ Authorization = "Bearer $token" }
$catalog = Invoke-RestMethod "$base/v1/models/installed" -Headers $headers
$catalog.models | Select-Object id, category, format_type, supported_tasks, experimental
# Save the Body Payload as request.json after selecting your installed model.
# Replace /v1/embeddings below with the POST endpoint you are testing.
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
| Extract document text/chunks | `POST /v1/ingest`, omit model | Native PDF/OCR/DOCX/DOC/text extraction |
| Document or image model processing | `POST /v1/ingest`, select model | Text embedding model for native chunks; ONNX image graph for images/PDF pages; generative GGUF vision model for text |

The Explorer Model dropdown writes the installed ID into POST payloads, or into `?model=` on GET voice/language discovery. Ingest also offers Native extraction, which omits the model. Body Payload has a separate example menu for document/image/text formats and model modes. TTS voices come from the selected package. The editor restores locally saved bodies; use Reset Payload to load a new shipped example. Hand-edited JSON remains authoritative; verify the model/source before sending.

`/v1/ingest` extracts chunks and can run a selected embedding/vision model; `/v1/embeddings` accepts prepared text or image inputs directly. Your application stores/indexes/searches the returned vectors. These routes do not automatically create a persistent RAG database. Retention for chat, speech, STT and direct embeddings is specified in **minutes** (`0` unloads, positive expires, `-1` retains). Ingest has no per-request retention field; its selected model uses the existing owner's cached lifecycle.

---

## 📦 Dedicated API Documentation Modules

### 1. Inference & Multimodal Execution
* [**`POST /v1/chat/completions`**](./inference/chat_completions.md) — Conversational dialogue, multimodal vision/audio input, streaming SSE, and reasoning tokens.
* [**`POST /v1/chat/cancel`**](./inference/chat_cancel.md) — Cancel active stream mid-generation by unique `stream_id`.
* [**`POST /v1/chat/skip-reasoning`**](./inference/chat_skip_reasoning.md) — Fast-forward reasoning chains and jump directly to the final answer.
* [**`GET/DELETE /v1/chat/memory`, `POST /v1/chat/memory/settings`, `DELETE /v1/chat/memory/{chat_id}`**](./inference/chat_memory.md) — Chat memory: each chat's KV cache saved to disk and restored when the chat continues.
* [**`POST /v1/embeddings`**](./inference/embeddings.md) — High-dimensional vector generation with GGUF/ONNX routing.
* [**`POST /v1/audio/speech`**](./inference/audio_speech.md) — Text-to-Speech (TTS) via Kokoro/Piper.
* [**`POST /v1/audio/execute`**](./inference/audio_speech.md) — Shared speech task route: text `input_source` synthesizes speech, audio `input_source` is transcribed.
* [**`GET /v1/audio/voices`**](./inference/audio_speech.md#which-voices-a-model-has) — Voices of a TTS model and the language each speaks.
* [**`POST /v1/audio/transcriptions`**](./inference/audio_transcriptions.md) — Speech-to-Text (STT) via Whisper encoder/decoder.
* [**`GET /v1/audio/languages`**](./inference/audio_transcriptions.md#which-languages-a-model-can-transcribe) — Languages an STT model can transcribe (for `language` / `languages`).
* [**`POST /v1/ingest`**](./inference/ingest.md) — Document & text vector ingestion pipeline.
* [**`POST /v1/rerank`**](./inference/rerank.md) — Cross-encoder document re-ranking.

### 2. Model Management
* [**`GET /v1/models` & `GET /api/tags`**](./models/list_models.md) — List installed and active models.
* [**`POST /models/load`**](./models/load_model.md) — Dynamic VRAM slot allocation and hot-swapping.
* [**`POST /api/pull`**](./models/pull_model.md) — Download GGUF/ONNX weights directly from HuggingFace.

### 3. Native High-Speed Filesystem & Workspace Jail
* [**`POST /v1/fs/*` & `/v1/workspace`**](./fs/filesystem.md) — Direct NVMe/SSD read, write, list, delete, copy, rename, and directory operations bounded inside the canonical workspace jail with ancestor walking.

### 4. Agent Governance & Human-in-the-Loop (HITL) Gate
* [**`GET/POST /v1/system/permission`**](./system/permission.md) — Security mode (`strict`, `sandboxed`, `full_access`), active workspace root, and API auth configuration.
* [**`GET /v1/system/permission/pending`**](./system/permission.md#31-list-pending-permission-requests-get-v1systempermissionpending) — Poll paused agent execution tasks.
* [**`POST /v1/system/permission/approve`**](./system/permission.md#32-approve-permission-request-post-v1systempermissionapprove) — Approve paused tool/filesystem payload and resume execution.
* [**`POST /v1/system/permission/reject`**](./system/permission.md#33-reject-permission-request-post-v1systempermissionreject) — Reject sensitive tool payload with feedback.

### 5. System & Hardware Control
* [**`GET /health`**](./system/health.md) — Engine health and uptime verification.
* [**`GET /info`**](./system/info.md) — Architectural pillars and version truth.
* [**`GET /v1/system/info`**](./system/info_hardware.md) — Calibrated silicon truth, CPU GFLOPS, measured RAM bandwidth, and GPU PCIe specs.
* [**`GET /v1/system/control`**](./system/info_hardware.md) — Hardware profile probe (alias to `/v1/system/info`).
* [**`GET /v1/system/live`**](./system/live.md) — Real-time telemetry (Host RAM/VRAM, per-core CPU %, engine process RSS memory).
* [**`POST /v1/system/plan`**](./system/plan.md) — Autonomous model execution planning engine (evaluates the 5 llama.cpp execution candidates against live probed hardware).
* [**`POST /v1/system/compare`**](./system/compare.md) — Dynamic latency comparison engine (alias to `/v1/system/plan`).
* [**`POST /v1/system/cmd`**](./system/cmd.md) — Protected host shell execution bridge (Bearer Token mandatory).
* [**`POST /v1/system/storage/temp_media/clean`**](./system/storage_clean.md) — Temporary media and cache cleanup.
* [**`GET/POST /v1/system/gguf_config`**](./system/gguf_config.md) — GGUF metadata and sampler parameters.
* [**`GET/POST /v1/system/onnx_config`**](./system/onnx_config.md) — ONNX runtime tuning and thread configuration.

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

All error responses return structured JSON with standard HTTP status codes:

```json
{
  "error": "Unauthorized",
  "message": "Missing or invalid Authorization token. Session or API Bearer token is required for sensitive system operations."
}
```
