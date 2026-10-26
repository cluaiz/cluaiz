//! `kv.*`: the saved KV cache per chat. Its settings are `config` section `kv`. Any client that
//! sends a `session_id` with its chat requests gets the same saved KV.

use crate::cep::CallCtx;
use cluaiz_cep::{Call, Meta, NoParams, Registry, Scope};
use cluaiz_runtime::chat_memory;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChatId {
    /// The chat: the `session_id` its chat requests send
    chat_id: String,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "kv.status",
            Meta::new(Scope::Read, "Where saved KV caches are kept, the disk they use, and the saved chats (most recently used first)")
                .example(json!({}), json!({ "directory": "C:/Users/me/.cluaiz/kv_cache/chats", "bytes": 28774640,
                    "chats": [{ "chat_id": "c1", "bytes": 28774640, "tokens": 743, "positions": 743,
                        "model": "C:/Users/me/.cluaiz/models/chat/gemma-4-e4b/gemma-4-E4B-it-Q4_K_M.gguf",
                        "key": "C:/Users/me/.cluaiz/models/chat/gemma-4-e4b/gemma-4-E4B-it-Q4_K_M.gguf|ctx=Auto|ngl=None",
                        "saved_at": 1792951094, "last_used": 1792951094 }] })),
            |_: CallCtx, _: Call, _: NoParams| async move {
                let usage = chat_memory::usage();
                Ok(json!({ "directory": chat_memory::directory(), "bytes": usage.bytes, "chats": usage.chats }))
            },
        )
        .add(
            "kv.delete",
            Meta::new(Scope::Write, "Forgets one chat's saved KV").destructive().example(json!({ "chat_id": "c1" }), json!({ "chat_id": "c1", "deleted": true })),
            |_: CallCtx, _: Call, p: ChatId| async move { Ok(json!({ "chat_id": p.chat_id, "deleted": chat_memory::delete(&p.chat_id) })) },
        )
        .add(
            "kv.clear",
            Meta::new(Scope::Write, "Forgets every chat's saved KV").destructive().example(json!({}), json!({ "deleted": 3 })),
            |_: CallCtx, _: Call, _: NoParams| async move { Ok(json!({ "deleted": chat_memory::clear() })) },
        );
}
