# `POST /models/load` API Specification

Mounts a model from the local vault into active RAM/VRAM compute memory.

---

## 📡 HTTP Request

```http
POST /models/load
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
  "status": "loaded",
  "model_id": "bonsai:8b",
  "time_elapsed_ms": 1420
}
```
