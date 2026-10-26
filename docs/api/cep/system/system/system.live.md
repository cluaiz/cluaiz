---
method: "system.live"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Current process, memory, CPU and GPU telemetry"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.live",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cpu": {
        "brand": "AMD Ryzen 7 7435HS",
        "frequency_mhz": 2002,
        "logical_threads": 16,
        "physical_cores": 8
      },
      "gpus": [
        {
          "compute_capability": "8.9",
          "index": 0,
          "name": "NVIDIA GeForce RTX 3050 A Laptop GPU",
          "pcie_gen": 2,
          "pcie_width": 8,
          "vendor": "NVIDIA",
          "vram_free_bytes": 2816151552,
          "vram_total_bytes": 4292870144
        }
      ],
      "memory": {
        "available_gb": 7.47,
        "total_gb": 23.69,
        "used_gb": 16.23
      },
      "process_id": 18356
    }
  }
errors: []
---

# system.live

Current telemetry of the Engine process and the machine: memory in use, CPU, and each GPU's free VRAM. Meant for a status bar or a dashboard that polls it.

## What it does

Reads the current values from the hardware prober each call. Nothing is cached between calls, and nothing is changed.

## Result

| Field | Meaning |
|---|---|
| `process_id` | The Engine's process id |
| `memory` | `total_gb`, `available_gb`, `used_gb` (system RAM) |
| `cpu` | `brand`, `physical_cores`, `logical_threads`, `frequency_mhz` |
| `gpus[]` | Per GPU: `index`, `vendor`, `name`, `vram_total_bytes`, `vram_free_bytes`, `compute_capability`, PCIe generation and width |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.live","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
