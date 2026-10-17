# Chat Memory API

Chat memory keeps each chat's processed context (its llama.cpp KV cache) on disk, so a chat continues at once after another chat, a model reload or an engine restart, instead of processing its whole history again. It is llama-server's `--slot-save-path`, managed per chat by the engine.

## How it works

1. Send the same `session_id` with every `POST /v1/chat/completions` request of a chat.
2. After each answer the engine saves that chat's KV cache to `<cluaiz home>/kv_cache/chats/` (`<chat>.kv`, a JSON record, and for chats with images or audio a media layout). Saving happens after the answer has been sent.
3. When a request arrives for a chat that is not the one in the context, its saved KV is restored and only the new tokens are processed. `engine.context.cache_n` in the response shows how many came from the cache.

A record is restored only for the same model file, load settings and KV cache type, into a context with room for its positions. Otherwise the history is processed as usual. Requests with `temporary_chat` set (private chats) are never saved. Requests without a `session_id` are not saved.

Measured (Bonsai 4B Q1_0, CPU): a saved chat of 380–933 tokens used 28–73 MB and saved in 31–80 ms. After an engine restart, continuing a chat processed 143 new tokens instead of 674 (time to first token 8.8 s instead of 28.8 s). Size grows with the model's KV per token and the chat length: from a few MB to about 1 GB for a long chat on a larger model.

## Settings

Stored in `settings.json` → `chat_memory`:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `enabled` | boolean | `true` | Save and restore chat memory |
| `max_disk_gb` | number ≥ 0 | `5` | Disk the saved chats may use; the least recently used are removed first |
| `max_age_days` | integer | `30` | Saved chats unused for this long are removed; `0` = only the disk limit |

## Endpoints

All require `Authorization: Bearer <session-or-api-token>`.

### `GET /v1/chat/memory`

Settings, disk used and saved chats, most recently used first.

```json
{
  "settings": { "enabled": true, "max_disk_gb": 5.0, "max_age_days": 30 },
  "directory": "C:\\Users\\me\\.cluaiz\\kv_cache\\chats",
  "bytes": 84857483,
  "chats": [
    {
      "chat_id": "chat_A",
      "model": "C:\\Users\\me\\.cluaiz\\models\\chat\\bonsai-4b-gguf-q1_0\\Bonsai-4B-Q1_0.gguf",
      "key": "<model path>|<load settings>|file=<bytes>:<modified>|kv=Kv8",
      "tokens": 933,
      "positions": 933,
      "bytes": 73103783,
      "saved_at": 1792213500,
      "last_used": 1792213500
    }
  ]
}
```

`tokens` is the text token list (0 for a chat with images or audio, which keeps a chunk layout instead); `positions` is the KV positions the chat fills; times are Unix seconds.

### `POST /v1/chat/memory/settings`

Changes any of the settings. The disk and age limits apply at once, so lowering `max_disk_gb` can remove saved chats immediately.

```json
{ "max_disk_gb": 2, "max_age_days": 7 }
```

```json
{ "settings": { "enabled": true, "max_disk_gb": 2.0, "max_age_days": 7 }, "bytes": 84857483 }
```

| HTTP | Meaning |
|---|---|
| 200 | Saved |
| 400 | `max_disk_gb` negative or not a number (`invalid_request_error`) |
| 422 | Unknown field or wrong type |

### `DELETE /v1/chat/memory/{chat_id}`

Forgets one chat's saved memory. Call it when the chat itself is deleted. `deleted` is false when there was nothing saved.

```json
{ "chat_id": "chat_A", "deleted": true }
```

### `DELETE /v1/chat/memory`

Forgets every saved chat. Chats and messages kept by the client are not affected.

```json
{ "deleted": 2 }
```

## Example

```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:8000/v1/chat/memory
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"max_disk_gb": 2}' http://localhost:8000/v1/chat/memory/settings
curl -X DELETE -H "Authorization: Bearer $TOKEN" http://localhost:8000/v1/chat/memory/chat_A
```
