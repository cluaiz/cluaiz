---
method: "chat.cancel"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "Stops a running chat stream"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "reason"
    type: "string | null"
    req: false
    desc: "Written to the Engine log with the cancel (optional)"
    example: null
  - name: "stream_id"
    type: "string"
    req: true
    desc: "The `id` of the running chat stream (from its SSE chunks)"
    example: "chatcmpl-1a2b"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "chat.cancel",
    "params": {
      "stream_id": "chatcmpl-1a2b"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "status": "cancelled",
      "stream_id": "chatcmpl-1a2b"
    }
  }
errors:
  - code: -32004
    type: "stream_not_found"
    when: "No running stream with this id (it ended, or never started)"
---

# chat.cancel

Stops a chat answer that is streaming now. `stream_id` is the `id` of the stream's chunks (`chatcmpl-…`).

## What it does

1. Finds the running stream; one that ended or never started answers `-32004` `stream_not_found`.
2. Sets its cancel flag and the Engine's native cancel signal, so generation stops between two tokens and the stream ends.

`reason` only goes to the Engine log.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.cancel","params":{"stream_id":"chatcmpl-1a2b","reason":"user pressed stop"}}'
```

Leave out the `Authorization` header when API authentication is off.
