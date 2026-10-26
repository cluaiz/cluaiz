---
method: "memory.export"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "A chat's events and cards, for an app's own store"
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
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.export",
    "params": {
      "chat_id": "c1"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cards": [],
      "chat_id": "c1",
      "events": [],
      "format": "cluaiz-chat-export",
      "version": 1
    }
  }
errors:
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.export

A chat's events and cards as one JSON document, for an app that keeps chats in its own store. [`memory.import`](memory.import.md) takes the `events` back.

Answers `{"format": "cluaiz-chat-export", "version": 1, "chat_id", "events": [...], "cards": [...]}`. The events have the same shape as in [`memory.events`](memory.events.md) and as the `memory_events` a chat response carries.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.export","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
