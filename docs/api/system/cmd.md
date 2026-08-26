# Local System Command Execution API

`POST /v1/system/cmd`

Executes shell commands locally. STRICTLY bound to `127.0.0.1` (Localhost) to prevent unauthorized remote code execution.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/system/cmd`
* **Content-Type:** `application/json`

---

## 📦 Request Payload

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | `string` | **Yes** | The raw shell command to execute locally on the host machine. |

### Example Request Body
```json
{
  "command": "echo Hello Cluaiz"
}
```

---

## 💻 Code Examples

### 1. cURL
```bash
curl -X POST http://localhost:8080/v1/system/cmd \
  -H "Content-Type: application/json" \
  -d '{"command": "echo Hello Cluaiz"}'
```

### 2. Python
```python
import requests

response = requests.post(
    "http://localhost:8080/v1/system/cmd",
    json={"command": "echo Hello Cluaiz"}
)
print(response.json())
```

### 3. JavaScript / Node.js
```javascript
const response = await fetch("http://localhost:8080/v1/system/cmd", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ command: "echo Hello Cluaiz" })
});
const data = await response.json();
console.log(data);
```

### 4. Rust
```rust
let client = reqwest::Client::new();
let res = client.post("http://localhost:8080/v1/system/cmd")
    .json(&serde_json::json!({ "command": "echo Hello Cluaiz" }))
    .send()
    .await?;
println!("{:#?}", res.json::<serde_json::Value>().await?);
```

---

## 📤 Response Format

```json
{
  "status": "success",
  "output": "Hello Cluaiz\n",
  "exit_code": 0
}
```
