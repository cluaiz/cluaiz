# Live Stream Steering & Control API

`POST /v1/chat/skip-reasoning` & `POST /v1/chat/cancel`

Real-time programmatic steering controls for active inference streams, allowing clients to skip thinking chains or cancel generation mid-stream using isolated, multi-tenant `stream_id` keys.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Paths:**
  * `/v1/chat/skip-reasoning` — Signals the active chat stream to close its `<think>` chain-of-thought block immediately and stream the direct final answer.
  * `/v1/chat/cancel` — Cancels an active chat stream mid-generation using its unique `stream_id`.

---

## 📦 Request Payload Format

Both endpoints accept a JSON payload specifying the target `stream_id` (obtained from the first SSE chunk of `/v1/chat/completions`):

```json
{
  "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"
}
```

---

## 💻 Code Examples

### 1. Skip Reasoning (cURL)

```bash
curl -X POST http://localhost:8000/v1/chat/skip-reasoning \
  -H "Content-Type: application/json" \
  -d '{"stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"}'
```

### 2. Cancel Generation (cURL)

```bash
curl -X POST http://localhost:8000/v1/chat/cancel \
  -H "Content-Type: application/json" \
  -d '{"stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"}'
```

### 3. Python (Requests)

```python
import requests

stream_id = "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"

# Skip reasoning during an active streaming completion
res_skip = requests.post(
    "http://localhost:8000/v1/chat/skip-reasoning",
    json={"stream_id": stream_id}
)
print("Skip Reasoning Response:", res_skip.json())

# Abort active token generation
res_cancel = requests.post(
    "http://localhost:8000/v1/chat/cancel",
    json={"stream_id": stream_id}
)
print("Cancel Response:", res_cancel.json())
```

---

## 📤 Response Format (`/v1/chat/skip-reasoning`)

```json
{
  "status": "skipped",
  "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1",
  "message": "Skip reasoning signal dispatched successfully."
}
```

---

## 📤 Response Format (`/v1/chat/cancel`)

```json
{
  "status": "cancelled",
  "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1",
  "message": "Stream cancellation signal dispatched successfully."
}
```
