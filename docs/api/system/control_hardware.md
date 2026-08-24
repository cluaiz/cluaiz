# Hardware & System Control Telemetry API

`GET /v1/system/control` & `GET /hardware`

Probes physical host hardware in real-time, returning CPU topologies, RAM bandwidth, GPU accelerators, VRAM capacity, and active driver subsystems.

---

## 📌 Endpoint Information

* **HTTP Method:** `GET`
* **Paths:**
  * `/v1/system/control` — Complete system context, identity, and silicon truth.
  * `/hardware` — Hardware detector summary probe.

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X GET http://localhost:8000/v1/system/control
```

### 2. Python (Requests)

```python
import requests

response = requests.get("http://localhost:8000/v1/system/control")
data = response.json()

if data.get("status") == "success":
    silicon = data.get("control", {}).get("silicon_truth", {})
    cpu = silicon.get("cpu", {})
    memory = silicon.get("memory", {})
    gpus = silicon.get("accelerators", {}).get("gpus", [])
    
    print(f"CPU: {cpu.get('brand')} ({cpu.get('physical_cores')} cores / {cpu.get('logical_threads')} threads)")
    print(f"RAM: {memory.get('total_capacity_gb', 0):.2f} GB (Available: {memory.get('available_capacity_gb', 0):.2f} GB)")
    for gpu in gpus:
        print(f"GPU: {gpu.get('vendor')} {gpu.get('model')} | VRAM: {gpu.get('vram_total_gb', 0):.2f} GB")
```

---

## 📤 Response Format (`/v1/system/control`)

```json
{
  "status": "success",
  "control": {
    "identity": {
      "machine_name": "DESKTOP-DEV",
      "os_target": "windows",
      "architecture": "x86_64",
      "kernel_version": "10.0.22631"
    },
    "context": {
      "cluaiz_root": "C:/Users/Aryan/.cluaiz"
    },
    "silicon_truth": {
      "cpu": {
        "brand": "13th Gen Intel(R) Core(TM) i7-13700H",
        "architecture": "x86_64",
        "numa_nodes": 1,
        "physical_cores": 14,
        "logical_threads": 20,
        "base_clock_mhz": 2400.0,
        "boost_clock_mhz": 5000.0,
        "isa_features": ["avx2", "fma", "sse4.2"]
      },
      "memory": {
        "total_capacity_gb": 31.75,
        "available_capacity_gb": 16.42,
        "type_name": "DDR5",
        "speed_mts": 4800.0,
        "bandwidth_gbps": 76.8,
        "is_unified_memory": false
      },
      "accelerators": {
        "gpus": [
          {
            "vendor": "NVIDIA",
            "model": "NVIDIA GeForce RTX 4070 Laptop GPU",
            "vram_total_gb": 8.0,
            "vram_available_gb": 5.4,
            "compute_capability": "8.9",
            "is_unified_with_system": false
          }
        ],
        "npus": [],
        "tpus": []
      }
    }
  }
}
```
