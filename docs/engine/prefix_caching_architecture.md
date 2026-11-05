# KV Cache Reuse (Prefix Caching)

When a chat continues, most of the new prompt is the same as the last one. The Engine keeps the
KV cache of the last generation and computes only the tokens that changed. This works like
llama-server's prompt cache, `--cache-reuse`, context shift and `--slot-save-path`.

Code: `engine/llama/src/native/stream.rs` (matching), `engine/llama/src/native/context.rs` (chunk
reuse, shift), `engine/runtime/src/chat_memory.rs` (save to disk).

## 1. In memory: the same model, the next turn

1. **Common prefix.** The new prompt's tokens are compared with the tokens the KV cache holds,
   from the start. The matching part is kept. Only the rest is processed.
2. **Chunk reuse** (`kv_reuse`). When old turns were dropped to fit the context, the history after
   the drop is still in the cache, only at other positions. The Engine moves those shared chunks
   instead of computing them again (llama-server `--cache-reuse`).
   - Auto: chunks only when this request dropped history; otherwise prefix only.
   - `kv_reuse: "prefix"` or `"chunk"` in a chat request forces one rule.
   - After an edit inside the prompt (for example a tool was unloaded), prefix-only is right: the
     text after the edit must be computed again, or the removed text keeps its influence.
3. **Checkpoints.** Models with sliding-window or recurrent memory cannot cut their cache at any
   position. The Engine then restarts at the newest checkpoint inside the shared prefix, or
   processes the prompt again.
4. **Tool step continuation.** After a tool call, the tool's result is appended to what the
   context already holds. When it does not fit, the oldest tokens after BOS are shifted out
   (llama-server's context shift).
5. **Media.** A request with images or audio is processed without text prefix reuse.

Every response reports what happened (`cache_n`, `processed_tokens`, `cause`: `cold`, `append`,
`edit`, `history_drop`, `shift`, `restart`, `media`). See
[chat_completions.md](../api/inference/chat_completions.md).

A compressed KV (`Kv8`, `Kv4`) never moves positions: llama.cpp's K-shift has no quantized form and
would stop the whole process. Such a chat frees a quarter of the context when it is full, and shared
chunks after an edit are processed again, as with a sliding-window model.

## 2. On disk: after a model reload, another chat or a restart

After each answer the chat's KV cache is saved to `<CLUAIZ_HOME>/kv_cache/chats/<chat>.kv` with a
JSON record. When the chat continues later, the KV is read back instead of processing the whole
history.

- A saved KV is restored only for the exact model file and load settings it was made with.
- Limits come from `settings.json` → `chat_memory`: `enabled` (default `true`), `max_disk_gb`
  (default `5.0`, least recently used chats are deleted first) and `max_age_days` (default `30`).
- Private chats (`temporary_chat`) are not saved.
- CEP `kv.status`, `kv.delete`, `kv.clear` show and remove saved chats.

## 3. Measured

KV equality tests (`test/context-runtime/kv_equality.py`, Windows test machine): the answer with
a reused or restored KV matched the cold-prompt answer in KV1 (10/10) and KV2 (10/10); KV3 was
equal.
