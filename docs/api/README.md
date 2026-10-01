# Cluaiz Engine REST API Reference

Welcome to the Cluaiz Engine REST API documentation. The engine provides high-performance, hardware-agnostic local inference for Large Language Models (LLMs), Vision-Language Models (VLMs), Embedding models, Speech-to-Text (STT), Text-to-Speech (TTS), and a secure agentic execution runtime with workspace containment.

---

## 🌐 Base URL & Endpoints

| Protocol | Default Local Endpoint | Description |
|:---|:---|:---|
| **HTTP REST** | `http://localhost:8080` | Core inference and management endpoints |
| **WebSocket / SSE** | `http://localhost:8080/v1/chat/completions` | Real-time token streaming with HITL approval events |
| **Developer Hub** | `http://localhost:8080/devhub` | Embedded management and testing interface |

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
| **Sensitive Filesystem** | `POST /v1/fs/read`, `/v1/fs/write`, `/v1/fs/delete` | **Bearer Token Required (401)** | **Bearer Token Required (401)** |
| **Host Terminal Exec** | `POST /v1/system/cmd` | **Bearer Token Required (401)** | **Bearer Token Required (401)** |

> **Critical Security Guarantee:** Sensitive routes (`/v1/fs/*` and `/v1/system/cmd`) **NEVER** run without a valid Bearer token, protecting developer workstations from malicious cross-origin scripts even when general inference is open.

---

## 📦 Dedicated API Documentation Modules

### 1. Inference & Multimodal Execution
* [**`POST /v1/chat/completions`**](./inference/chat_completions.md) — Conversational dialogue, multimodal vision/audio input, streaming SSE, and reasoning tokens.
* [**`POST /v1/chat/cancel`**](./inference/chat_cancel.md) — Cancel active stream mid-generation by unique `stream_id`.
* [**`POST /v1/chat/skip-reasoning`**](./inference/chat_skip_reasoning.md) — Fast-forward reasoning chains and jump directly to the final answer.
* [**`POST /v1/embeddings`**](./inference/embeddings.md) — High-dimensional vector generation with GGUF/ONNX routing.
* [**`POST /v1/audio/speech`**](./inference/audio_speech.md) — Text-to-Speech (TTS) via Kokoro/Piper.
* [**`POST /v1/audio/transcriptions`**](./inference/audio_transcriptions.md) — Speech-to-Text (STT) via Whisper encoder/decoder.
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
* [**`POST /v1/system/cmd`**](./system/cmd.md) — Protected host shell execution bridge (Bearer Token mandatory).
* [**`GET /v1/system/control`**](./system/control_hardware.md) — Hardware probe (GPU VRAM, CUDA/DirectML/Metal status).
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
