# System Health API

`GET /health`

Checks the core engine status, active memory allocations, and continuous uptime of Cluaiz.

---

## 📌 Endpoint Information

* **HTTP Method:** `GET`
* **Path:** `/health`

---

## 💻 Code Examples

### 1. cURL
```bash
curl -X GET http://localhost:8080/health
```

### 2. Python
```python
import requests

response = requests.get("http://localhost:8080/health")
print(response.json())
```

### 3. JavaScript / Node.js
```javascript
const response = await fetch("http://localhost:8080/health");
const data = await response.json();
console.log(data);
```

### 4. Rust
```rust
let res = reqwest::get("http://localhost:8080/health").await?;
println!("{:#?}", res.json::<serde_json::Value>().await?);
```

---

## 📤 Response Format

```json
{
  "status": "healthy",
  "uptime_ms": 145020,
  "components": {
    "db": "healthy",
    "gpu": "healthy"
  }
}
```
