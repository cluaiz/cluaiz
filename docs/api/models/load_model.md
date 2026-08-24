# Load Model API

`POST /models/load`

Locates model weights and queues kernel instantiation into active compute runtime.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/models/load`
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`model_id`** | String | **Yes** | — | Unique identifier or filename of the model to load into active runtime memory. |

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/models/load \
  -H "Content-Type: application/json" \
  -d '{
    "model_id": "qwen2.5-7b-instruct"
  }'
```

### 2. Python (Requests)

```python
import requests

payload = {
    "model_id": "qwen2.5-7b-instruct"
}

response = requests.post("http://localhost:8000/models/load", json=payload)
data = response.json()
print("Load status:", data.get("status"))
print("Message:", data.get("message"))
```

---

## 📤 Response Format

### Success (`200 OK`)

```json
{
  "status": "success",
  "message": "Model 'qwen2.5-7b-instruct' located at 'C:/Users/Aryan/.cluaiz/models/chat/qwen2.5-7b-instruct/qwen2.5-7b-instruct-q4_k_m.gguf'. Kernel instantiation queued."
}
```

### Model Not Found (`200 OK`)

```json
{
  "status": "error",
  "message": "Model 'unknown-model' not found in vault or not downloaded."
}
```
