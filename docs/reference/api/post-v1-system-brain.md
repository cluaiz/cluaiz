# `POST /v1/system/brain` API Specification

Toggles the FFI connection status to the local or remote database daemon.

---

## 📡 HTTP Request

```http
POST /v1/system/brain
Content-Type: application/json
```

### JSON Request Payload:
```json
{
  "state": true,
  "address": "127.0.0.1:8080"
}
```

---

## 📡 Response Schema

```json
{
  "brain_connected": true,
  "address": "127.0.0.1:8080"
}
```
