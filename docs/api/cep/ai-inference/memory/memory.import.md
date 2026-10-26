---
method: "memory.import"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Gives back a chat an app keeps itself (storage external), held in RAM only"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
  - name: "events"
    type: "any[]"
    req: true
    desc: "The events of `memory.export` (or the `memory_events` an app collected)"
    example: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.import",
    "params": {
      "chat_id": "c1",
      "events": []
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cards": 0,
      "chat_id": "c1",
      "events": 0,
      "storage": "external"
    }
  }
errors:
  - code: -32006
    type: "too_large"
    when: "More than 200 000 events"
  - code: -32602
    type: "invalid_params"
    when: "An event does not have the event shape of `memory.export`"
---

# memory.import

Gives a chat back to the Engine from an app that stores chats itself ("storage external"). The events are held in RAM only: recall, the Chat Map and the next chat requests use them, and nothing is written to the Engine's disk.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## What it does

1. Refuses more than 200 000 events, and events that do not have the export shape.
2. Marks the chat as external and loads the events into memory, replacing what was there.
3. Rebuilds the chat's Chat Map. Answers the number of `events` and `cards`.

An app sends this when it opens a chat; afterwards it stores the `memory_events` each chat response returns. Not an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.import","params":{"chat_id":"c1","events":[{"v":1,"id":"e1","chat":"c1","seq":1,"ts":"2026-10-25T10:00:00.000Z","kind":"message","role":"user","text":"Hi"}]}}'
```

Leave out the `Authorization` header when API authentication is off.
