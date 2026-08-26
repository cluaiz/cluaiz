# Engine Info API

`GET /info`

Retrieves deep architectural information, versioning, and foundational pillars of the Cluaiz runtime engine.

---

## 📌 Endpoint Information

* **HTTP Method:** `GET`
* **Path:** `/info`

---

## 💻 Code Examples

### 1. cURL
```bash
curl -X GET http://localhost:8080/info
```

### 2. Python
```python
import requests

response = requests.get("http://localhost:8080/info")
print(response.json())
```

### 3. JavaScript / Node.js
```javascript
const response = await fetch("http://localhost:8080/info");
const data = await response.json();
console.log(data);
```

### 4. Rust
```rust
let res = reqwest::get("http://localhost:8080/info").await?;
println!("{:#?}", res.json::<serde_json::Value>().await?);
```

---

## 📤 Response Format

```json
{
  "engine": "Cluaiz Inference Engine",
  "version": "0.1.0",
  "build_date": "2026-08-26",
  "capabilities": ["vision", "vector_search", "cel", "prefix_caching"]
}
```
