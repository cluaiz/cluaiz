# `POST /models/download` API Specification

Asynchronously downloads a selected model from the registry.

---

## 📡 HTTP Request

```http
POST /models/download
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "model_id": "bonsai:8b"
}
```

---

## 📡 Response Schema

```json
{
  "task_id": "dl_bonsai_8b_f7g8",
  "status": "downloading",
  "bytes_total": 4820000000
}
```
