# `GET /models/tags` API Specification

Queries the Cluaize centralized model registry to find available choices.

---

## 📡 HTTP Request

```http
GET /models/tags
```

---

## 📡 Response Schema

```json
{
  "models": [
    {
      "name": "bonsai:8b",
      "architecture": "Llama",
      "quant": "q4_k_m"
    }
  ]
}
```
