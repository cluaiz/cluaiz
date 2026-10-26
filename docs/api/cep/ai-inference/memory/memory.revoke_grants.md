---
method: "memory.revoke_grants"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Asks again for this chat's tools (or one tool)"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
  - name: "tool"
    type: "string | null"
    req: false
    desc: "Only this tool; all of the chat's grants when absent"
    example: "web-search"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.revoke_grants",
    "params": {
      "chat_id": "c1",
      "tool": "web-search"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "removed": 1
    }
  }
errors: []
---

# memory.revoke_grants

Removes a chat's tool grants (all of them, or one tool's), so the tools ask again. The removal is written to the chat's event log, so a restart does not bring the grants back. Answers how many were `removed`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.revoke_grants","params":{"chat_id":"c1","tool":"web-search"}}'
```

Leave out the `Authorization` header when API authentication is off.
