# `POST /v1/permission/update` API Specification

Modifies parameters inside the local `Permission.json` configuration file.

---

## 📡 HTTP Request

```http
POST /v1/permission/update
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "key": "firewall_mode",
  "value": "strict"
}
```

---

## 📡 Response Schema

```json
{
  "status": "updated",
  "key": "firewall_mode",
  "value": "strict"
}
```
