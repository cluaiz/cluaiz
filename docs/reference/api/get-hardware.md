# `GET /hardware` API Specification

Fetches current hardware specifications and active device status.

---

## 📡 HTTP Request

```http
GET /hardware
```

---

## 📡 Response Schema

```json
{
  "cpu": "Intel Core i7-12700H",
  "ram_total_gb": 16.0,
  "ram_free_gb": 4.2,
  "accelerator": "CUDA (NVIDIA GeForce RTX 3050 Laptop GPU)",
  "vram_total_gb": 4.0,
  "vram_free_gb": 1.1
}
```
