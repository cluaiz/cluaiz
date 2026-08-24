# Download / Pull Model API

`POST /api/pull`

Asynchronously triggers background download and registration of model weights into local hardware storage.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/api/pull`
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`model_id`** | String | **Yes** | — | Unique model identifier or HuggingFace repository ID (e.g. `"qwen2.5-7b-instruct"` or `"bge-m3"`). |

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/api/pull \
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

response = requests.post("http://localhost:8000/api/pull", json=payload)
print(response.json())
```

---

## 📤 Response Format

```json
{
  "status": "success",
  "message": "Model pull for 'qwen2.5-7b-instruct' queued in background."
}
```
