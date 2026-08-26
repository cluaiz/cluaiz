# Engine Security & Permission Configuration API

`GET /v1/system/permission` & `POST /v1/system/permission`

Manages system security policies, WASM runtime boundaries, input vectorization flags, and real-time telemetry streaming permissions.

---

## 📌 Endpoint Information

* **HTTP Methods:** `GET`, `POST`
* **Path:** `/v1/system/permission`
* **Content-Type:** `application/json`

---

## 📦 Request Payload (`POST`)

| Field | Type | Description |
|---|---|---|
| `wasm_firewall` | `string` | Firewall security policy mode (`"strict"` or `"permissive"`). |
| `vectorize_user_input` | `boolean` | Whether to automatically generate and store embeddings for conversation turns. |
| `stream_telemetry` | `boolean` | Whether to append real-time hardware telemetry chunks to SSE streams. |
| `model_header_info` | `boolean` | Whether to stream probed model architecture headers to the frontend. |

### Example Request Body
```json
{
  "wasm_firewall": "strict",
  "vectorize_user_input": true,
  "stream_telemetry": false,
  "model_header_info": true
}
```

---

## 💻 Code Examples

### 1. Get Permissions (cURL)
```bash
curl -X GET http://localhost:8080/v1/system/permission
```

### 2. Update Permissions (cURL)
```bash
curl -X POST http://localhost:8080/v1/system/permission \
  -H "Content-Type: application/json" \
  -d '{"wasm_firewall": "strict", "vectorize_user_input": true, "stream_telemetry": false}'
```

### 3. Python
```python
import requests

# Fetch permissions
res = requests.get("http://localhost:8080/v1/system/permission")
print(res.json())

# Update permissions
update_payload = {
    "wasm_firewall": "strict",
    "vectorize_user_input": True,
    "stream_telemetry": False
}
res_post = requests.post("http://localhost:8080/v1/system/permission", json=update_payload)
print(res_post.json())
```

---

## 📤 Response Format

```json
{
  "wasm_firewall": "strict",
  "vectorize_user_input": true,
  "stream_telemetry": false,
  "model_header_info": true
}
```
