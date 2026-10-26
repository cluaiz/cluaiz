---
method: "tasks.list"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tasks"
summary: "Your tasks that have not expired, newest first"
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
    "method": "tasks.list",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "tasks": [
        {
          "created_at": "2026-10-25T10:30:00Z",
          "last_updated_at": "2026-10-25T10:30:02Z",
          "method": "audio.run",
          "poll_ms": 500,
          "status": "working",
          "task_id": "t_5f0c…",
          "ttl_ms": 3600000
        }
      ]
    }
  }
errors: []
---

# tasks.list

The caller's tasks that have not expired, newest first. A task is kept in memory for an hour after it starts (a restart forgets every task), and a caller can run at most 8 tasks at once.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tasks.list","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
