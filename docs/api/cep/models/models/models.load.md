---
method: "models.load"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Loads an installed GGUF model using the existing Engine host"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "model"
    type: "string"
    req: true
    desc: "An installed model id (from `models.list`)"
    example: "installed-model"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.load",
    "params": {
      "model": "installed-model"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "loaded_ms": 125,
      "model": "installed-model"
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or its weight file is missing"
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty or looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32007
    type: "not_implemented"
    when: "The model is not a GGUF (ONNX models load on their first request)"
  - code: -32010
    type: "model_load_failed"
    when: "llama.cpp could not load it (memory, a broken file, an unsupported architecture)"
---

# models.load

Loads an installed GGUF model into the Engine now, so the first chat request does not wait for it. Chat requests also load their model on their own; this only does it ahead of time.

## What it does

1. Finds the model and checks its weight file against the caller's roots.
2. Refuses a non-GGUF model with `-32007`: ONNX models load on their first request.
3. Plans the load with the saved settings (the same plan as [`system.plan`](../../system/system/system.plan.md)) and loads it, replacing the model loaded before.
4. Answers the model id and `loaded_ms`. A model that is already loaded with the same plan is reused, so the answer comes at once.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.load","params":{"model":"qwen3-4b-gguf-Q4_K_M"}}'
```

Leave out the `Authorization` header when API authentication is off.
