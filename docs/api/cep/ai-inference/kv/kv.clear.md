---
method: "kv.clear"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "kv"
summary: "Forgets every chat's saved KV"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "kv.clear",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "deleted": 3
    }
  }
errors: []
---

# kv.clear

Forgets every chat's saved KV cache. Chat histories stay. Answers how many chats' caches were `deleted`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"kv.clear","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
