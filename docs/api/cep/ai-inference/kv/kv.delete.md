---
method: "kv.delete"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "kv"
summary: "Forgets one chat's saved KV"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the `session_id` its chat requests send"
    example: "c1"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "kv.delete",
    "params": {
      "chat_id": "c1"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "deleted": true
    }
  }
errors: []
---

# kv.delete

Forgets one chat's saved KV cache. The chat's history stays; its next request processes the history again. Answers `deleted: false` when nothing was saved.

The saved KV cache of a chat lets its next request continue where the last one stopped instead of processing the whole history again. Its settings are `config` section `kv` (`enabled`, `max_disk_gb`, `max_age_days`).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"kv.delete","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
