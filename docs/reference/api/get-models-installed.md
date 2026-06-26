# `GET /models/installed` API Specification

Returns a list of all model GGUF binaries downloaded in the local vault.

---

## 📡 HTTP Request

```http
GET /models/installed
```

---

## 📡 Response Schema

```json
[
  {
    "model_id": "bonsai:8b",
    "type": "chat",
    "size_bytes": 4820000000,
    "path": "C:\\Users\\Aryan\\.cluaize\\models\\chat\\bonsai-8b.gguf"
  }
]
```
