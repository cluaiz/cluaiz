//! Chat memory API: saved KV caches per chat (`GET/DELETE /v1/chat/memory`, settings).
//! Any client that sends a `session_id` with its chat requests gets the same memory.

use axum::{extract::Path, Json};
use cluaiz_runtime::chat_memory;
use serde::Deserialize;
use serde_json::{json, Value};

/// GET /v1/chat/memory: the settings, disk used and saved chats (most recently used first).
pub async fn get_chat_memory() -> Json<Value> {
    let settings = cluaiz_core::Settings::current().chat_memory;
    let usage = chat_memory::usage();
    Json(json!({
        "settings": settings,
        "directory": chat_memory::directory(),
        "bytes": usage.bytes,
        "chats": usage.chats,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMemorySettingsPatch {
    pub enabled: Option<bool>,
    pub max_disk_gb: Option<f64>,
    pub max_age_days: Option<u32>,
}

/// POST /v1/chat/memory/settings: changes the given fields; the limits apply at once.
pub async fn update_chat_memory_settings(
    Json(patch): Json<ChatMemorySettingsPatch>,
) -> (axum::http::StatusCode, Json<Value>) {
    let mut settings = cluaiz_core::Settings::current();
    if let Some(enabled) = patch.enabled {
        settings.chat_memory.enabled = enabled;
    }
    if let Some(gb) = patch.max_disk_gb {
        if !gb.is_finite() || gb < 0.0 {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(
                    json!({ "error": { "message": "max_disk_gb must be 0 or more", "type": "invalid_request_error" } }),
                ),
            );
        }
        settings.chat_memory.max_disk_gb = gb;
    }
    if let Some(days) = patch.max_age_days {
        settings.chat_memory.max_age_days = days;
    }
    let chat_memory = settings.chat_memory.clone();
    if let Err(e) = cluaiz_core::Settings::update(settings) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "message": e.to_string(), "type": "engine_error" } })),
        );
    }
    chat_memory::enforce_limits(&chat_memory, None);
    (
        axum::http::StatusCode::OK,
        Json(json!({ "settings": chat_memory, "bytes": chat_memory::usage().bytes })),
    )
}

/// DELETE /v1/chat/memory/{chat_id}: forgets one chat's saved KV (e.g. when the chat is deleted).
pub async fn delete_chat_memory(Path(chat_id): Path<String>) -> Json<Value> {
    Json(json!({ "chat_id": chat_id, "deleted": chat_memory::delete(&chat_id) }))
}

/// DELETE /v1/chat/memory: forgets every saved chat.
pub async fn clear_chat_memory() -> Json<Value> {
    Json(json!({ "deleted": chat_memory::clear() }))
}
