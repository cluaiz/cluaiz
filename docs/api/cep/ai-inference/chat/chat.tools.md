---
method: "chat.tools"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "The tools attached to a chat, with their turns left"
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
    "method": "chat.tools",
    "params": {
      "chat_id": "c1"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "active_tools": [
        {
          "id": "web-search",
          "turns": 3
        }
      ],
      "chat_id": "c1"
    }
  }
errors: []
---

# chat.tools

The tools attached to a chat, with how many answers each has left.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## Entry fields

| Field | Meaning |
|---|---|
| `id` | The tool |
| `turns` | `-1`: stays; `0`: unloaded after the next answer; `N`: answers left |
| `inline` | Loaded by the model with `load_tool` (it lives in the conversation, not in the system prompt) |
| `pinned` | Never unloaded for being idle |
| `idle` | Answers in a row that did not use it |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.tools","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
