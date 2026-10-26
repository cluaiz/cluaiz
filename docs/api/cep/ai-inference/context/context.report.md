---
method: "context.report"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "context"
summary: "One of a chat's last 20 context reports (kept in memory)"
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
  - name: "request_id"
    type: "string"
    req: true
    desc: "One id from `context.reports` (the chat completion's `id`)"
    example: "chatcmpl-1a2b"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "context.report",
    "params": {
      "chat_id": "c1",
      "request_id": "chatcmpl-1a2b"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "context_report": {
        "timing": {
          "preflight_ms": 23
        }
      },
      "request_id": "chatcmpl-1a2b"
    }
  }
errors:
  - code: -32004
    type: "report_not_found"
    when: "No such report: only the newest 20 per chat are kept, in memory, until a restart"
---

# context.report

One kept context report: the `tools` part (suggestions, loaded tools, their token cost), the `plan` part (room, kept history, compaction, recall, policy used) and `timing` (milliseconds for each preparation step) of one chat request.

`request_id` is the chat completion's `id` (from [`context.reports`](context.reports.md)).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"context.report","params":{"chat_id":"c1","request_id":"chatcmpl-1a2b"}}'
```

Leave out the `Authorization` header when API authentication is off.
