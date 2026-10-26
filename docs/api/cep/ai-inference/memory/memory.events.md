---
method: "memory.events"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "A chat's event log, oldest first"
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
  - name: "from_seq"
    type: "integer | null"
    req: false
    desc: "First seq to return (default 1)"
    example: 1
  - name: "limit"
    type: "integer | null"
    req: false
    desc: "Most events to return (default 500, at most 5000)"
    example: 2
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.events",
    "params": {
      "chat_id": "c1",
      "from_seq": 1,
      "limit": 2
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "events": [
        {
          "data": {
            "role": "user",
            "text": "Hi"
          },
          "kind": "message",
          "seq": 1
        }
      ],
      "next_from_seq": 3,
      "skipped_lines": 0,
      "total": 40
    }
  }
errors:
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.events

A chat's event log, oldest first, a page at a time. The event log is the chat's memory on disk: every message, tool call and result, card, note and compaction, in order.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

## Paging

`from_seq` (default 1) and `limit` (default 500, at most 5000). When a full page came back, `next_from_seq` is the `from_seq` of the next page; otherwise it is `null`.

## Result

| Field | Meaning |
|---|---|
| `events[]` | `seq`, `ts` (RFC 3339), `kind` (`message`, `tool_call`, `tool_result`, `card_enrichment`, `note`, `context_edit`…), `id`, `chat`, and the kind's own fields (`role`, `text`, `tool`, `args`, `result`…) |
| `total` | Events in the log |
| `skipped_lines` | Lines of the log file that could not be read (a torn write after a crash) |
| `next_from_seq` | The next page, or `null` |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.events","params":{"chat_id":"c1","from_seq":1,"limit":200}}'
```

Leave out the `Authorization` header when API authentication is off.
