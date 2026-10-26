---
method: "tasks.result"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tasks"
summary: "A task's final result or error; waits up to wait_ms"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "task_id"
    type: "string"
    req: true
    desc: "The `task.task_id` a task method answered with"
    example: "t_5f0c…"
  - name: "wait_ms"
    type: "integer | null"
    req: false
    desc: "How long to wait for the end, at most 30000 ms; 0 or absent = do not wait"
    example: 30000
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tasks.result",
    "params": {
      "task_id": "t_5f0c…",
      "wait_ms": 30000
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "any": "the result the method returns"
    }
  }
errors:
  - code: -32004
    type: "task_not_found"
    when: "No task with this id for this caller, or it expired"
  - code: -32009
    type: "timeout"
    when: "The task is still running after `wait_ms`: ask again"
  - code: -32008
    type: "cancelled"
    when: "The task was cancelled"
---

# tasks.result

The task's final result, or its error, exactly as the method would have answered directly. With `wait_ms`, it waits up to that long (at most 30 000 ms) for the task to end.

## What it answers

- The task ended well: its result.
- The task failed: its error (for example `-32010` with the method's `data.type`).
- The task was cancelled: `-32008`.
- Still running after `wait_ms`: `-32009`; ask again.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tasks.result","params":{"task_id":"t_5f0c…","wait_ms":30000}}'
```

Leave out the `Authorization` header when API authentication is off.
