# `POST /chat` API Specification

Streams token-by-token inference output using Server-Sent Events (SSE).

---

## 📡 HTTP Request

```http
POST /chat
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "prompt": "Write a Rust quicksort function",
  "temperature": 0.2,
  "max_tokens": 1024
}
```

---

## 📡 Response Schema

Returns a Server-Sent Events (SSE) stream (`text/event-stream`).

### Event Payload:
```text
data: {"token": "fn", "done": false}
data: {"token": " ", "done": false}
data: {"token": "quicksort", "done": false}
data: {"token": "", "done": true}
```
