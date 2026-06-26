# `POST /v1/booster/update` API Specification

Modifies parameters inside the active `system_booster.json` file.

---

## 📡 HTTP Request

```http
POST /v1/booster/update
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "key": "kv_quant",
  "value": "kv8"
}
```

---

## 📡 Response Schema

```json
{
  "status": "updated",
  "key": "kv_quant",
  "value": "kv8"
}
```
