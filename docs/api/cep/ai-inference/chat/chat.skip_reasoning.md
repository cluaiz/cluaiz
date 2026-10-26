---
method: "chat.skip_reasoning"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "Ends the model's reasoning; the same stream continues with the answer"
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
    "method": "chat.skip_reasoning",
    "params": {
      "stream_id": "chatcmpl-1a2b"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "status": "skip_requested",
      "stream_id": "chatcmpl-1a2b"
    }
  }
errors:
  - code: -32004
    type: "stream_not_found"
    when: "No running stream with this id (it ended, or never started)"
  - code: -32005
    type: "reasoning_unavailable"
    when: "The stream's model has no native reasoning control, or thinking is off"
---

# chat.skip_reasoning

Ends the model's thinking in a running stream; the same stream continues with the answer. Use it for a "skip thinking" button.

The stream must have native reasoning control enabled (a reasoning model with thinking on); otherwise `-32005` `reasoning_unavailable`. The request is picked up by the running generation, which stops thinking and continues with the answer in the same stream.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.skip_reasoning","params":{"stream_id":"chatcmpl-1a2b"}}'
```

Leave out the `Authorization` header when API authentication is off.
