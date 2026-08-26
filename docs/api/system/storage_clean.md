# Temporary Media Storage Cleanup API

`POST /v1/system/storage/temp_media/clean`

Purges orphaned temporary multimodal media files, cached blobs, and stale audio artifacts stored locally in `.cluaiz/storage/temp_media`.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/system/storage/temp_media/clean`

---

## 💻 Code Examples

### 1. cURL
```bash
curl -X POST http://localhost:8080/v1/system/storage/temp_media/clean
```

### 2. Python
```python
import requests

response = requests.post("http://localhost:8080/v1/system/storage/temp_media/clean")
print(response.json())
```

### 3. JavaScript / Node.js
```javascript
const response = await fetch("http://localhost:8080/v1/system/storage/temp_media/clean", {
  method: "POST"
});
const data = await response.json();
console.log(data);
```

### 4. Rust
```rust
let client = reqwest::Client::new();
let res = client.post("http://localhost:8080/v1/system/storage/temp_media/clean")
    .send()
    .await?;
println!("{:#?}", res.json::<serde_json::Value>().await?);
```

---

## 📤 Response Format

```json
{
  "status": "success",
  "freed_bytes": 104857600,
  "cleaned_files_count": 14,
  "message": "Temporary media storage purged successfully."
}
```
