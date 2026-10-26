---
method: "system.ps"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Loaded model execution plan and hardware snapshot"
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
    "method": "system.ps",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "active_processes": [
        {
          "context_size": 8192,
          "engine": "llama-engine (static rlib)",
          "model_id": "qwen3-4b-gguf-Q4_K_M",
          "n_gpu_layers": 37,
          "pid": "18356",
          "tier": "Gpu",
          "vram_gb": 3.2
        }
      ],
      "hardware_snapshot": {
        "architecture": "x86_64",
        "cpu": {},
        "gpus": [],
        "memory": {},
        "storage": {}
      }
    }
  }
errors: []
---

# system.ps

The model the Engine has loaded right now, with the execution plan it was loaded with, and the hardware snapshot. `cluaiz ps` shows the same.

## What it does

Reads the Engine host's current plan. When no model is loaded, `active_processes` is empty. The model runs inside the Engine process (llama.cpp is linked in), so `pid` is the Engine's own process id.

## Result

| Field | Meaning |
|---|---|
| `active_processes[]` | At most one entry: `model_id`, `vram_gb` (the VRAM budget of the plan), `context_size`, `tier`, `n_gpu_layers`, `engine`, `pid` |
| `hardware_snapshot` | The same hardware profile as [`system.info`](system.info.md) |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.ps","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
