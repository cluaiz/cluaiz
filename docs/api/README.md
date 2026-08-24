# Cluaiz Engine REST API Reference

Welcome to the Cluaiz Engine REST API documentation. The engine provides high-performance, hardware-agnostic local inference for Large Language Models (LLMs), Vision-Language Models (VLMs), Embedding models, Speech-to-Text (STT), and Text-to-Speech (TTS).

---

## 🌐 Base URL & Endpoints

| Protocol | Default Local Endpoint |
|:---|:---|
| **HTTP REST** | `http://localhost:8000` |
| **WebSocket / SSE** | `http://localhost:8000/v1/chat/completions` |
| **Developer Hub** | `http://localhost:8000/devhub` (Embedded UI) |

---

## 🔒 Authentication

By default, local developer access is open on `localhost`. For secured deployments, pass your API key via the standard `Authorization` header:

```http
Authorization: Bearer YOUR_API_KEY
```

---

## 📦 API Modules

### 1. [Inference & Multimodal Execution](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/docs/api/inference/chat_completions.md)
* **`POST /v1/chat/completions`** — Text, Vision (`image_url`), Audio (`audio_url`), Streaming SSE, and Chain-of-Thought (`think_mode`).
* **`POST /v1/embeddings`** — High-dimensional vector generation with automatic GGUF/ONNX routing and L2 normalization.
* **`POST /v1/audio/speech`** — Ultra-fast local Text-to-Speech (TTS) via Kokoro-82M and Piper.
* **`POST /v1/audio/transcriptions`** — Native Speech-to-Text (STT) via Whisper encoder/decoder.

### 2. [Model Management](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/docs/api/models/list_models.md)
* **`GET /v1/models/installed`** & **`GET /api/tags`** — List installed and active models.
* **`POST /models/load`** — Dynamic VRAM slot allocation and hot-swapping.
* **`POST /models/unload`** — Release VRAM back to the operating system.
* **`POST /api/pull`** — Download GGUF/ONNX weights directly from HuggingFace.
* **`DELETE /v1/models/{model_id}`** — Safe deletion of model files and metadata.

### 3. [System & Hardware Telemetry](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/docs/api/system/info_health.md)
* **`GET /health`** & **`GET /info`** — Engine health, active slots, and version truth.
* **`GET /v1/system/control`** & **`GET /hardware`** — Silicon Truth probe (GPU VRAM, CUDA/DirectML/Metal status, CPU threads).
* **`GET/POST /v1/system/gguf_config`** — Hardware offload, context sizing, and sampler defaults.
* **`GET/POST /v1/system/onnx_config`** — Graph optimization, execution providers, and thread tuning.

---

## ⚡ OpenAI Client Quickstart (Python)

Cluaiz Engine is 100% compatible with the official OpenAI Python SDK:

```python
from openai import OpenAI

# Connect to local Cluaiz Engine
client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="cluaiz-local" # Any string when running locally
)

response = client.chat.completions.create(
    model="auto",
    messages=[
        {"role": "system", "content": "You are a helpful AI assistant."},
        {"role": "user", "content": "Explain the concept of quantum superposition in 2 sentences."}
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
    "message": "Model 'qwen2.5-7b' is not loaded in VRAM.",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```
