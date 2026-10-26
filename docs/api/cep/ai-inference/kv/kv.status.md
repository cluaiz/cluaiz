---
method: "kv.status"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "kv"
summary: "Where saved KV caches are kept, the disk they use, and the saved chats (most recently used first)"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "kv.status",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "bytes": 28774640,
      "chats": [
        {
          "bytes": 28774640,
          "chat_id": "c1",
          "key": "C:/Users/me/.cluaiz/models/chat/gemma-4-e4b/gemma-4-E4B-it-Q4_K_M.gguf|ctx=Auto|ngl=None",
          "last_used": 1792951094,
          "model": "C:/Users/me/.cluaiz/models/chat/gemma-4-e4b/gemma-4-E4B-it-Q4_K_M.gguf",
          "positions": 743,
          "saved_at": 1792951094,
          "tokens": 743
        }
      ],
      "directory": "C:/Users/me/.cluaiz/kv_cache/chats"
    }
  }
errors: []
---

# kv.status

Where saved KV caches are kept, how much disk they use, and the saved chats, most recently used first.

The saved KV cache of a chat lets its next request continue where the last one stopped instead of processing the whole history again. Its settings are `config` section `kv` (`enabled`, `max_disk_gb`, `max_age_days`).

## Entry fields

`chat_id`, `bytes`, `tokens` and `positions` (how much of the chat it holds), `model` (the weight file it belongs to), `key` (model plus load settings: a cache only fits the same model loaded the same way), `saved_at`, `last_used` (Unix seconds).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"kv.status","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
