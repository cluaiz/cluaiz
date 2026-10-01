# Local System Command Execution API Reference

> **Base Route:** `POST /v1/system/cmd`  
> **Security Protocol:** Permanent Bearer Token Authentication (Session Token or API Key)  
> **Component:** `cluaiz_api::handlers::system::execute_cmd`, `cluaiz_api::auth`

---

## 1. Security & Threat Model

The `POST /v1/system/cmd` endpoint allows authorized clients and agents to execute host operating system shell commands (`cmd.exe` / `powershell` on Windows, `sh` / `bash` on Linux/macOS).

### 🔒 Mandatory Token Protection

Because shell execution has host-level authority:
1. **Permanent Bearer Auth:** `/v1/system/cmd` **NEVER** runs unauthenticated. Even if the global "Require API Authentication" toggle is turned OFF for local LLM prototyping, this route **strictly requires** an `Authorization: Bearer <token>` header.
2. **Drive-by Protection:** This ensures malicious browser scripts, third-party web apps, or cross-site request attacks cannot trigger remote code execution against the developer's machine.
3. **Accepted Tokens:**
   * **Engine Session Secret:** Loaded from `~/.cluaiz/session.token` (auto-generated at boot with restricted `0600` POSIX permissions).
   * **User API Key:** Configured under `api_auth.tokens` in `permission.json` (e.g., `sk-cluaiz-...`).

---

## 2. Endpoint Specification

* **HTTP Method:** `POST`
* **Path:** `/v1/system/cmd`
* **Headers:**
  * `Authorization: Bearer <token>` (**Required**)
  * `Content-Type: application/json`

### Request Payload

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | `string` | **Yes** | The shell command string to execute on the host machine. |

#### Example Request Body
```json
{
  "command": "git status --short"
}
```

---

## 3. Response Format

### Success (`200 OK`)
```json
{
  "status": "success",
  "output": " M src/auth.rs\n?? docs/api/fs/\n",
  "exit_code": 0
}
```

### Unauthorized (`401 Unauthorized`)
Returned if the `Authorization` header is missing, malformed, or contains an invalid token:
```json
{
  "error": "Unauthorized",
  "message": "Missing or invalid Authorization token. Session or API Bearer token is required for sensitive system operations."
}
```

---

## 4. Code Examples

### 1. cURL
```bash
# Using configured API key
curl -X POST http://localhost:8080/v1/system/cmd \
  -H "Authorization: Bearer sk-cluaiz-e46c11e877b20afb49c4df0fcfa44351" \
  -H "Content-Type: application/json" \
  -d '{"command": "cargo --version"}'
```

### 2. Python
```python
import os
import requests

API_KEY = os.getenv("CLUAIZ_API_KEY", "sk-cluaiz-your-token-here")

response = requests.post(
    "http://localhost:8080/v1/system/cmd",
    headers={
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    },
    json={"command": "dir"}
)
print("Exit Code:", response.json().get("exit_code"))
print("Output:\n", response.json().get("output"))
```

### 3. JavaScript / TypeScript (Node.js)
```typescript
const token = process.env.CLUAIZ_API_KEY || "sk-cluaiz-your-token-here";

const response = await fetch("http://localhost:8080/v1/system/cmd", {
  method: "POST",
  headers: {
    "Authorization": `Bearer ${token}`,
    "Content-Type": "application/json",
  },
  body: JSON.stringify({ command: "node --version" }),
});

const data = await response.json();
console.log(data);
```

### 4. Rust (Reqwest)
```rust
let client = reqwest::Client::new();
let res = client.post("http://localhost:8080/v1/system/cmd")
    .header("Authorization", "Bearer sk-cluaiz-your-token-here")
    .json(&serde_json::json!({ "command": "cargo test" }))
    .send()
    .await?;

let body: serde_json::Value = res.json().await?;
println!("Execution Output:\n{}", body["output"]);
```
