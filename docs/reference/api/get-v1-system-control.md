# `GET /v1/system/control` API Specification

Returns the node configuration values stored in `system_control.json`.

---

## 📡 HTTP Request

```http
GET /v1/system/control
```

---

## 📡 Response Schema

```json
{
  "node_id": "cluaiz-node-x1",
  "active_model": "bonsai:8b",
  "user_identity": {
    "name": "Operator",
    "purpose": "PRODUCTION"
  },
  "hardware_governance": {
    "vram_limit_gb": 12.0,
    "cpu_thread_limit": 8,
    "allow_speculative_decoding": true,
    "fallback_to_cpu": true
  },
  "network": {
    "api_host": "127.0.0.1",
    "api_port": 3000,
    "enable_cors": true
  }
}
```
