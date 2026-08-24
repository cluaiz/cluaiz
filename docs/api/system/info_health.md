# System Info & Health API

`GET /health` & `GET /info`

Returns the operational status, engine version, and architecture telemetry of the Cluaiz Inference Engine.

---

## 📌 Endpoint Information

* **HTTP Method:** `GET`
* **Paths:**
  * `/health` — Lightweight liveness probe.
  * `/info` — Engine metadata and architectural pillars.

---

## 💻 Code Examples

### 1. cURL (Health Check)

```bash
curl -X GET http://localhost:8000/health
```

### 2. Python (Requests)

```python
import requests

res_health = requests.get("http://localhost:8000/health")
print("Health:", res_health.json())

res_info = requests.get("http://localhost:8000/info")
print("Info:", res_info.json())
```

---

## 📤 Response Format (`/health`)

```json
{
  "status": "alive",
  "engine": "cluaiz Inference Engine",
  "version": "0.1.0",
  "message": "🚀 cluaiz is alive! All systems operational."
}
```

---

## 📤 Response Format (`/info`)

```json
{
  "engine": "cluaiz",
  "full_name": "cluaiz Inference Engine",
  "version": "0.1.0",
  "pillars": {
    "api": "Gateway — HTTP server on port 8000 (this!)",
    "kernel": "Brain — Decision-making & orchestration",
    "storage": "Sidecars — 5 Official DB engines (Mongo, Neo4j, ClickHouse, Qdrant, MinIO)",
    "engines": "Muscles — C++ model inference via llama.cpp FFI"
  },
  "philosophy": "Nothing Need. Just cluaiz.",
  "banned": [
    "Python",
    "Docker",
    "npm",
    "pip"
  ]
}
```
