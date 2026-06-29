# `POST /v1/db/execute` API Specification

Forwards database query statements to the local/remote `cluaizdb` daemon.

---

## 📡 HTTP Request

```http
POST /v1/db/execute
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "query": "find User(status: 'active')"
}
```

---

## 📡 Response Schema

```json
{
  "results": [
    {
      "id": "usr_9f8g",
      "status": "active"
    }
  ]
}
```
