---
method: "chat.set_tools"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "Attaches, updates or detaches a chat's tools"
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
    desc: "The chat (`session_id` of its chat requests)"
    example: "c1"
  - name: "detach"
    type: "string[]"
    req: false
    desc: "Tool ids to detach"
    example: ["old-tool"]
  - name: "tools"
    type: "any[]"
    req: false
    desc: "Tools to attach or update: a tool id (its default turns), or `{\"id\", \"turns\"}`"
    example: ["web-search",{"id":"calc","turns":2}]
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "chat.set_tools",
    "params": {
      "chat_id": "c1",
      "detach": [
        "old-tool"
      ],
      "tools": [
        "web-search",
        {
          "id": "calc",
          "turns": 2
        }
      ]
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
errors:
  - code: -32602
    type: "invalid_params"
    when: "An entry of `tools` is neither a tool id nor `{\"id\", \"turns\"}`"
---

# chat.set_tools

Attaches, updates or detaches a chat's tools. An attached tool is offered to the model with its whole schema and instructions in that chat.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## Params

- `tools`: each entry is a tool id (attached with the tool's `default_turns`), or `{"id", "turns", "inline", "pinned"}`. `turns`: `-1` = stays, `0` = only the next answer, `N` = N answers.
- `detach`: tool ids to remove.

Answers the chat's tools after the change (as [`chat.tools`](chat.tools.md)).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.set_tools","params":{"chat_id":"c1","tools":["web-search",{"id":"filesystem","turns":-1,"pinned":true}],"detach":["calculator"]}}'
```

Leave out the `Authorization` header when API authentication is off.
