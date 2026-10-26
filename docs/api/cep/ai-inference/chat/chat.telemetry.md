---
method: "chat.telemetry"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "How a chat's context is used by its tools, for a model"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "The chat whose attached tools are counted (default: \"default\")"
    example: "c1"
  - name: "model"
    type: "string | null"
    req: false
    desc: "The model whose context window is measured (default: the active chat model, \"default\")"
    example: "qwen3-4b-gguf-Q4_K_M"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "chat.telemetry",
    "params": {
      "chat_id": "c1",
      "model": "qwen3-4b-gguf-Q4_K_M"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "model": "qwen3-4b-gguf-Q4_K_M",
      "telemetry": {}
    }
  }
errors: []
---

# chat.telemetry

How a chat's context window is used by its attached tools, for a model: the token budget the tools take next to the system prompt and the history. Meant for a context meter in a chat UI.

Without `model` and `chat_id`, the default model and the `default` chat are measured. The numbers are estimates from the tools' definitions; the exact split of one request is in its `context_report`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.telemetry","params":{"model":"qwen3-4b-gguf-Q4_K_M","chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
