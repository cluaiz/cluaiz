---
method: "context.preview"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "context"
summary: "The plan and tool choices a chat request would get, without generating (loads the model for its tokenizer)"
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
    desc: "The chat the request belongs to (its event log, ledger and loaded tools are used)"
    example: "c1"
  - name: "request"
    type: "any"
    req: true
    desc: "A chat completions request (model, messages, tools…); its `session_id` becomes `chat_id`"
    example: {"messages":[{"content":"Hi","role":"user"}],"model":"qwen3-4b-gguf-Q4_K_M"}
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "context.preview",
    "params": {
      "chat_id": "c1",
      "request": {
        "messages": [
          {
            "content": "Hi",
            "role": "user"
          }
        ],
        "model": "qwen3-4b-gguf-Q4_K_M"
      }
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "plan": {
        "n_ctx": 8192,
        "prompt_tokens": 412
      },
      "tools": {
        "cards": []
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`request` is not a chat request, or its `messages` is empty"
  - code: -32004
    type: "model_not_found"
    when: "The request's model is not installed"
---

# context.preview

The plan a chat request would get, without generating anything: how the context window would be shared between the system prompt, history, tools and the answer, which history would be kept or compacted, and which tools would be suggested or loaded.

## What it does

1. Takes `request` as a `/v1/chat/completions` body (`model`, `messages`, `tools`, `context_policy`…) for chat `chat_id`.
2. Loads the model if it is not loaded (the counts need its tokenizer).
3. Runs the same preparation a real request runs: ingest the messages, plan, recall, render the prompt, match tools.
4. Answers `plan` and `tools` of the context report. A compaction is reported but not saved, and the chat's tools do not change.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"context.preview","params":{"chat_id":"c1","request":{"model":"qwen3-4b-gguf-Q4_K_M","messages":[{"role":"user","content":"Summarise our deploy plan"}]}}}'
```

Leave out the `Authorization` header when API authentication is off.
