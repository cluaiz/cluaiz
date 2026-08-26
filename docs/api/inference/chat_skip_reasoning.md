# Skip Reasoning API

`POST /v1/chat/skip-reasoning`

Instructs an active chat stream to immediately exit its `<think>` reasoning block and fast-forward directly to outputting the final conversational answer.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/chat/skip-reasoning`
* **Content-Type:** `application/json`

---

## 📦 Request Payload

| Field | Type | Required | Description |
|---|---|---|---|
| `stream_id` | `string` | **Yes** | Unique stream identifier returned in the first SSE chunk of `/v1/chat/completions` (e.g., `chatcmpl-...`). |

### Example Request Body
```json
{
  "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"
}
```

---

## 💻 Code Examples

### 1. cURL
```bash
curl -X POST http://localhost:8080/v1/chat/skip-reasoning \
  -H "Content-Type: application/json" \
  -d '{"stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"}'
```

### 2. Python
```python
import requests

response = requests.post(
    "http://localhost:8080/v1/chat/skip-reasoning",
    json={"stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1"}
)
print(response.json())
```

### 3. JavaScript / Node.js
```javascript
const response = await fetch("http://localhost:8080/v1/chat/skip-reasoning", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ stream_id: "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1" })
});
const data = await response.json();
console.log(data);
```

### 4. Rust
```rust
let client = reqwest::Client::new();
let res = client.post("http://localhost:8080/v1/chat/skip-reasoning")
    .json(&serde_json::json!({ "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1" }))
    .send()
    .await?;
println!("{:#?}", res.json::<serde_json::Value>().await?);
```

---

## 📤 Response Format

### Success (200 OK)
```json
{
  "status": "skipped",
  "stream_id": "chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1",
  "message": "Skip reasoning signal dispatched successfully."
}
```

### Stream Not Found / Already Finished (404 Not Found)
```json
{
  "error": {
    "message": "Active stream 'chatcmpl-0c2510db1c8349dd8f193d8d69b7aee1' not found or already completed.",
    "type": "invalid_request_error",
    "code": "stream_not_found"
  }
}
```
