---
method: "memory.grants"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Tools the user allowed for this chat, with minutes left"
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
    "method": "memory.grants",
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
      "grants": [
        {
          "minutes_left": 42,
          "tool": "web-search"
        }
      ]
    }
  }
errors: []
---

# memory.grants

The tools the user allowed in this chat without asking again ("remember for this chat" on an approval), with the minutes left for each.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

Grants come from [`approvals.approve`](../../agents-tools/approvals/approvals.approve.md) with `remember: "chat"`. They are written to the chat's event log, so a restart keeps them until they expire.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.grants","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
