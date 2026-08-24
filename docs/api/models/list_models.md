# List Installed & Available Models API

`GET /v1/models/installed` & `GET /api/tags`

Returns a list of all locally installed GGUF and ONNX models synchronized on user hardware, including their category, parameters, quantization, supported tasks, and file metadata.

---

## 📌 Endpoint Information

* **HTTP Method:** `GET`
* **Paths:**
  * `/v1/models/installed` — Full synchronized registry metadata (Engine Standard).
  * `/api/tags` — Standard tag summary format.

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X GET http://localhost:8000/v1/models/installed
```

### 2. Python (Requests)

```python
import requests

response = requests.get("http://localhost:8000/v1/models/installed")
data = response.json()

if data.get("status") == "success":
    for model in data.get("models", []):
        meta = model.get("metadata", {})
        print(f"ID: {model['id']} | Category: {model.get('category')} | Format: {model.get('format_type')} | Params: {meta.get('parameters', 'N/A')}")
```

---

## 📤 Response Format (`/v1/models/installed`)

```json
{
  "status": "success",
  "count": 2,
  "models": [
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
}
```
