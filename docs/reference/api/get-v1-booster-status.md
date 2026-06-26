# `GET /v1/booster/status` API Specification

Returns the performance parameters mapped inside `system_booster.json`.

---

## 📡 HTTP Request

```http
GET /v1/booster/status
```

---

## 📡 Response Schema

```json
{
  "mode_run": "balance",
  "n_gpu_layers": 16,
  "kv_quant": "kv8",
  "context_shift": "standard",
  "flash_attention": "on",
  "speculative_decoding": "off"
}
```
