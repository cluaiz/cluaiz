---
method: "context.reports"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "context"
summary: "Ids of a chat's kept context reports, newest last"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat (`session_id` of its chat requests)"
    example: "c1"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "context.reports",
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
      "reports": [
        "chatcmpl-1a2b"
      ]
    }
  }
errors: []
---

# context.reports

The ids of a chat's kept context reports, newest last. The Engine keeps the report of the newest 20 requests of each chat, in memory only; a restart forgets them. Read one with [`context.report`](context.report.md).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"context.reports","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
