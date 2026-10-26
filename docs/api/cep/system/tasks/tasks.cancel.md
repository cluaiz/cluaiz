---
method: "tasks.cancel"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tasks"
summary: "Cancels a running task"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "task_id"
    type: "string"
    req: true
    desc: "The `task.task_id` a task method answered with"
    example: "t_5f0c…"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tasks.cancel",
    "params": {
      "task_id": "t_5f0c…"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "status": "cancelled",
      "status_message": "Cancelled by request"
    }
  }
errors:
  - code: -32004
    type: "task_not_found"
    when: "No task with this id for this caller, or it expired"
  - code: -32602
    type: "invalid_params"
    when: "The task has already ended"
---

# tasks.cancel

Cancels a running task. The task's status becomes `cancelled` at once and its work is told to stop; work that can stop between steps (a download, an audio run, an install) stops there. A task that already ended cannot be cancelled.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tasks.cancel","params":{"task_id":"t_5f0c…"}}'
```

Leave out the `Authorization` header when API authentication is off.
