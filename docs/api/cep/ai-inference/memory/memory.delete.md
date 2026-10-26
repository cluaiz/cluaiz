---
method: "memory.delete"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Deletes a chat's event log, ledger, saved KV, context reports and Chat Map"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
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
    "method": "memory.delete",
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
      "events": {
        "bytes": 2048,
        "files": 1
      },
      "folder_removed": true,
      "ledger": true,
      "saved_kv": {}
    }
  }
errors:
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be removed"
---

# memory.delete

Deletes everything the Engine keeps about a chat: its event log, ledger, saved KV cache, kept context reports, tool grants and Chat Map. Shared blobs (stored once by content, used by several chats) stay; an empty chat folder is removed.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## Result

`events` (`files`, `bytes` removed), `ledger: true`, `saved_kv` (what [`kv.delete`](../kv/kv.delete.md) answered), `folder_removed`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.delete","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
