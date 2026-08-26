# Cluaiz Engine REST API Reference

Welcome to the Cluaiz Engine REST API documentation. The engine provides high-performance, hardware-agnostic local inference for Large Language Models (LLMs), Vision-Language Models (VLMs), Embedding models, Speech-to-Text (STT), and Text-to-Speech (TTS).

---

## 🌐 Base URL & Endpoints

| Protocol | Default Local Endpoint |
|:---|:---|
| **HTTP REST** | `http://localhost:8080` |
| **WebSocket / SSE** | `http://localhost:8080/v1/chat/completions` |
| **Developer Hub** | `http://localhost:8080/devhub` (Embedded UI) |

---

## 🔒 Authentication

By default, local developer access is open on `localhost`. For secured deployments, pass your API key via the standard `Authorization` header:

```http
Authorization: Bearer YOUR_API_KEY
```

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

### 3. System & Hardware Control
* [**`GET /health`**](./system/health.md) — Engine health and memory status.
* [**`GET /info`**](./system/info.md) — Architectural info and version truth.
* [**`POST /v1/system/cmd`**](./system/cmd.md) — Localhost command execution bridge.
* [**`GET /v1/system/control`**](./system/control_hardware.md) — Silicon Truth probe (GPU VRAM, CUDA/DirectML/Metal status).
* [**`GET/POST /v1/system/permission`**](./system/permission.md) — Security boundaries, WASM firewalls, and telemetry settings.
* [**`POST /v1/system/storage/temp_media/clean`**](./system/storage_clean.md) — Temporary media and cache cleanup.
* [**`GET/POST /v1/system/gguf_config`**](./system/gguf_config.md) — GGUF metadata and sampler headers.
* [**`GET/POST /v1/system/onnx_config`**](./system/onnx_config.md) — ONNX runtime tuning and thread configuration.

---

## ⚡ OpenAI Client Quickstart (Python)

Cluaiz Engine provides drop-in OpenAI Python and TypeScript SDK compatibility:

```python
from openai import OpenAI

# Connect to local Cluaiz Engine
client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="cluaiz-local" # Any string when running locally
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
  "error": {
    "message": "Active stream 'chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1' not found or already completed.",
    "type": "invalid_request_error",
    "code": "stream_not_found"
  }
}
```
