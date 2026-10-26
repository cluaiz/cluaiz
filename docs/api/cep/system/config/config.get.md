---
method: "config.get"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "config"
summary: "The saved settings: one section, or all of them"
scope: "read"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: true
params:
  - name: "section"
    type: "\"gguf\" | \"onnx\" | \"optimization\" | \"context\" | \"storage\" | \"kv\" | \"engine\" | null"
    req: false
    desc: "One section; all of them when absent"
    example: "kv"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "config.get",
    "params": {
      "section": "kv"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "kv": {
        "enabled": true,
        "max_age_days": 30,
        "max_disk_gb": 10.0
      }
    }
  }
errors: []
---

# config.get

The saved settings, one section or all of them. The Engine's settings live in two files in its home, `settings.json` and `permission.json`; each section below is a part of one of them.

## Sections

| Section | File | Holds |
|---|---|---|
| `gguf` | settings.json | llama.cpp loading (`hardware_and_execution`: `n_gpu_layers`, `n_ctx`, `no_mmap`, `batch_size`, `ubatch_size`), the default samplers, and `think_mode` |
| `onnx` | settings.json | ONNX Runtime: execution provider, device, threads, memory arena |
| `optimization` | settings.json | The planner's inputs: context window, KV cache type, mmap, flash attention, buffers, thinking presets |
| `context` | settings.json | The context runtime's policy: recent turns, caps, compaction, indexer, notes, cross-chat search, tool format |
| `storage` | settings.json | `cleanup_policy` of temporary media |
| `kv` | settings.json | Saved KV cache per chat: `enabled`, `max_disk_gb`, `max_age_days` |
| `engine` | permission.json | Model slots, API host and port, `api_auth`, agent security mode, `exec` policy |

## Notes

- `gguf` answers the saved values, with `null` / `-1` for automatic, not the numbers the planner would pick. Saving the page back therefore keeps "automatic" automatic. Use [`system.plan`](../system/system.plan.md) to see the resolved numbers.
- Reading `engine` also empties model slots that name a model that is no longer installed, and saves that.
- Not an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"config.get","params":{"section":"kv"}}'
```

Leave out the `Authorization` header when API authentication is off.
