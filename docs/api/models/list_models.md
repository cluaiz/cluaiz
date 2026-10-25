# List Installed Models

CEP `models.list` · `GET /v1/models` (OpenAI) · `GET /api/tags` (Ollama)

Returns the locally installed GGUF and ONNX models, with their category, parameters, quantization, supported tasks and file metadata.

---

## 📌 Endpoint Information

* `models.list` (CEP, `POST /v1/cep`): the full registry entries, optionally filtered by `category`. The result is the array of entries.
* `GET /v1/models`: OpenAI's list format (`{"object": "list", "data": [...]}`).
* `GET /api/tags`: Ollama's tag summary format.

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/v1/cep -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.list","params":{}}'
```

### 2. Python (Requests)

```python
import requests

call = {"jsonrpc": "2.0", "id": 1, "method": "models.list", "params": {}}
models = requests.post("http://localhost:8000/v1/cep", json=call).json()["result"]
for model in models:
    meta = model.get("metadata", {})
    print(f"ID: {model['id']} | Category: {model.get('category')} | Format: {model.get('format_type')} | Params: {meta.get('parameters', 'N/A')}")
```

Add the header `Authorization: Bearer <token>` when API authentication is on.

---

## 📤 Response Format (`models.list` result)

Each model also has `experimental`: `true` for a text-to-speech family whose engine support is unfinished (Audio8, Supertonic, Matcha, CosyVoice, Chatterbox, OmniVoice; list in `engine/onnx/src/audio/tts/data/tts_families.json`). Such a model still runs when chosen by name, but the engine never picks it automatically, and clients should label it and keep it out of default choices.

```json
[
    {
      "id": "qwen2.5-7b-instruct",
      "category": "chat",
      "format_type": "gguf",
      "huggingface_repo": "Qwen/Qwen2.5-7B-Instruct-GGUF",
      "local_dir": "C:/Users/Aryan/.cluaiz/models/chat/qwen2.5-7b-instruct",
      "files": [
        {
          "name": "qwen2.5-7b-instruct-q4_k_m.gguf",
          "size_bytes": 4680000000,
          "is_primary": true
        }
      ],
      "supported_tasks": [
        "chat-completion"
      ],
      "requires_gpu": false,
      "experimental": false,
      "metadata": {
        "architecture": "qwen2",
        "parameters": "7B",
        "context_window": "32768",
        "quantization": "Q4_K_M",
        "bit_depth": "4-bit"
      }
    },
    {
      "id": "bge-m3",
      "category": "embedding",
      "format_type": "onnx",
      "huggingface_repo": "BAAI/bge-m3",
      "local_dir": "C:/Users/Aryan/.cluaiz/models/embedding/bge-m3",
      "files": [
        {
          "name": "model.onnx",
          "size_bytes": 1340000000,
          "is_primary": true
        }
      ],
      "supported_tasks": [
        "sentence-similarity",
        "feature-extraction"
      ],
      "requires_gpu": false,
      "metadata": {
        "architecture": "xlm-roberta",
        "parameters": "567M",
        "context_window": "8192",
        "quantization": "FP32",
        "bit_depth": "32-bit"
      }
    }
]
```
