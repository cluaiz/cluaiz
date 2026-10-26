---
method: "tasks.get"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tasks"
summary: "A task's status, progress and new events"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "after"
    type: "integer | null"
    req: false
    desc: "Events after this cursor (`next` of the last answer); 0 or absent = all kept events"
    example: 0
  - name: "task_id"
    type: "string"
    req: true
    desc: "The `task.task_id` a task method answered with"
    example: "t_5f0c…"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tasks.get",
    "params": {
      "after": 0,
      "task_id": "t_5f0c…"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "events": [],
      "next": 0,
      "progress": {
        "done": 1,
        "total": 3
      },
      "status": "working"
    }
  }
errors:
  - code: -32004
    type: "task_not_found"
    when: "No task with this id for this caller, or it expired"
---

# tasks.get

A task's status, its progress, and the events it produced after a cursor. Poll it while a task runs to show progress.

## What it answers

| Field | Meaning |
|---|---|
| `status` | `working`, `completed`, `failed` or `cancelled` |
| `progress` | `done`, `total`, `unit` (`bytes`, `sources`, `chats`…), `message`, when the method reports it |
| `events` | Output the task produced after `after`: an audio chunk, a download step… |
| `next` | The cursor to send as `after` next time |

A task keeps its newest 1000 events. Only the caller that started a task sees it; for anyone else it does not exist.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tasks.get","params":{"task_id":"t_5f0c…","after":0}}'
```

Leave out the `Authorization` header when API authentication is off.
