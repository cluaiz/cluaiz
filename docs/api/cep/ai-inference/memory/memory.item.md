---
method: "memory.item"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "The exact message behind a reference (m:<position>)"
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
  - name: "ref"
    type: "string"
    req: true
    desc: "A reference from a search result, `m:<position>`"
    example: "m:12"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.item",
    "params": {
      "chat_id": "c1",
      "ref": "m:12"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "card": {},
      "chat_id": "c1",
      "ref": "m:12",
      "text": "The deploy server listens on port 8443."
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`ref` does not look like m:<position>"
  - code: -32004
    type: "message_not_found"
    when: "No visible message at that position"
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.item

The exact message behind a reference from [`memory.search`](memory.search.md) or a recalled line: `ref` is `m:<position>`. Answers the message's `card` and its full `text`.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.item","params":{"chat_id":"c1","ref":"m:12"}}'
```

Leave out the `Authorization` header when API authentication is off.
