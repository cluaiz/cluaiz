# `GET /v1/permission` API Specification

Reads current node options stored in `Permission.json`.

---

## 📡 HTTP Request

```http
GET /v1/permission
```

---

## 📡 Response Schema

```json
{
  "firewall_mode": "strict",
  "enable_telemetry": false,
  "vectorize_user_input": true,
  "vectorize_ai_response": true,
  "chat_ttl_hours": 24,
  "default_chat_model": "bonsai:8b",
  "default_vector_model": "bge_m3:unknown:onnx:fp32"
}
```
