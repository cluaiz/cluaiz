# ONNX Engine Configuration API

`GET /v1/system/onnx_config` & `POST /v1/system/onnx_config`

Inspects and updates execution providers, thread pools, memory arenas, and graph optimizations for ONNX models.

---

## 📌 Endpoint Information

* **HTTP Methods:** `GET` (read configuration), `POST` (save configuration)
* **Path:** `/v1/system/onnx_config`
* **Content-Type:** `application/json`

---

## 💻 Code Examples

### 1. cURL (Read ONNX Config)

```bash
curl -X GET http://localhost:8000/v1/system/onnx_config
```

### 2. Python (Update Thread Settings)

```python
import requests

res = requests.get("http://localhost:8000/v1/system/onnx_config")
config = res.json()

config["intra_op_num_threads"] = 4
config["enable_mem_pattern"] = True
config["graph_optimization_level"] = "ORT_ENABLE_ALL"

save_res = requests.post("http://localhost:8000/v1/system/onnx_config", json=config)
print("Save result:", save_res.json())
```

---

## 📥 Configuration Schema

```json
{
  "n_gpu_layers": -1,
  "n_ctx": 0,
  "intra_op_num_threads": 0,
  "inter_op_num_threads": 0,
  "graph_optimization_level": "ORT_ENABLE_ALL",
  "enable_profiling": false,
  "enable_mem_pattern": true,
  "enable_cpu_mem_arena": true,
  "execution_mode": "ORT_SEQUENTIAL",
  "gpu_mem_limit_bytes": 0,
  "arena_extend_strategy": "kNextPowerOfTwo",
  "enable_ort_transformers_optimization": true,
  "kv_cache_data_type": "ort_fp16",
  "use_deterministic_compute": false,
  "user_moved_flags": {
    "think_mode": "Auto"
  }
}
```
