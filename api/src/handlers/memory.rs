//! Chat memory API: a chat's event log, its Chat Map (cards), search over it, export, and
//! deleting a chat's history with everything derived from it.

use super::chat::recall;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use cluaiz_agents::memory::{EventStore, JsonlStore};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": { "message": message.into(), "type": "invalid_request_error" } }))).into_response()
}

#[derive(Deserialize)]
pub struct EventsQuery {
    /// First seq to return (default 1)
    #[serde(default)]
    pub from_seq: Option<u64>,
    /// Most events to return (default 500, at most 5000)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// GET /v1/chat/{session_id}/events: the chat's event log, oldest first
pub async fn get_events(Path(chat): Path<String>, Query(query): Query<EventsQuery>) -> Response {
    match JsonlStore::shared().read(&chat) {
        Ok(log) => {
            let from = query.from_seq.unwrap_or(1);
            let limit = query.limit.unwrap_or(500).min(5000);
            let total = log.events.len();
            let events: Vec<_> = log.events.into_iter().filter(|e| e.seq >= from).take(limit).collect();
            let next = events.last().map(|e| e.seq + 1).filter(|_| events.len() == limit);
            Json(json!({ "chat_id": chat, "total": total, "skipped_lines": log.skipped_lines, "events": events, "next_from_seq": next })).into_response()
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// GET /v1/chat/{session_id}/index: the chat's cards (one per visible message; no text)
pub async fn get_index(Path(chat): Path<String>) -> Response {
    match recall::chat_map(&chat) {
        Some(map) => Json(json!({ "chat_id": chat, "cards": map.cards, "episodes": map.episodes })).into_response(),
        None => error(StatusCode::INTERNAL_SERVER_ERROR, "could not read the chat's event log"),
    }
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub chat_id: String,
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// POST /v1/recall/search: messages of a chat that fit a query (all of them are candidates)
pub async fn search(Json(request): Json<SearchRequest>) -> Response {
    let Some(map) = recall::chat_map(&request.chat_id) else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "could not read the chat's event log");
    };
    let hits = cluaiz_context::memory::recall(
        &map.cards,
        &map.index,
        &request.query,
        &HashSet::new(),
        chrono::Utc::now(),
        request.limit.unwrap_or(5).min(50),
    );
    let results: Vec<Value> = hits
        .iter()
        .map(|hit| {
            let card = &map.cards[hit.card];
            json!({ "ref": format!("m:{}", card.pos), "score": (hit.score * 100.0).round() / 100.0, "why": hit.why, "card": card })
        })
        .collect();
    Json(json!({ "chat_id": request.chat_id, "query": request.query, "results": results })).into_response()
}

#[derive(Deserialize)]
pub struct ItemQuery {
    pub chat_id: String,
}

/// GET /v1/recall/item/{ref}?chat_id=…: the exact message behind a reference (`m:<pos>`)
pub async fn get_item(Path(reference): Path<String>, Query(query): Query<ItemQuery>) -> Response {
    let Some(pos) = reference.strip_prefix("m:").and_then(|p| p.parse::<u64>().ok()) else {
        return error(StatusCode::BAD_REQUEST, "a reference looks like m:<position>");
    };
    let Some(map) = recall::chat_map(&query.chat_id) else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "could not read the chat's event log");
    };
    match map.cards.iter().position(|c| c.pos == pos) {
        Some(i) => Json(json!({ "chat_id": query.chat_id, "ref": reference, "card": map.cards[i], "text": map.text(i) })).into_response(),
        None => error(StatusCode::NOT_FOUND, "no visible message at that position"),
    }
}

/// GET /v1/chat/{session_id}/export: the chat's events and cards as JSON (for an app's own store)
pub async fn export(Path(chat): Path<String>) -> Response {
    let log = match JsonlStore::shared().read(&chat) {
        Ok(log) => log,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let cards = recall::chat_map(&chat).map(|map| map.cards.clone()).unwrap_or_default();
    Json(json!({ "format": "cluaiz-chat-export", "version": 1, "chat_id": chat, "events": log.events, "cards": cards })).into_response()
}

/// DELETE /v1/chat/{session_id}/history: removes the chat's event log, ledger, saved KV, kept
/// context reports and Chat Map. Shared blobs stay (they are stored by content).
pub async fn delete_history(Path(chat): Path<String>) -> Response {
    let events = match JsonlStore::shared().delete_chat(&chat) {
        Ok(report) => report,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    cluaiz_agents::ChatLedger::delete(&chat);
    let kv = cluaiz_runtime::chat_memory::delete(&chat);
    recall::forget(&chat);
    super::chat::context_plan::forget_reports(&chat);
    let folder = cluaiz_agents::memory::memory_root().join("chats").join(cluaiz_agents::memory::chat_stem(&chat));
    let folder_left = folder.exists() && std::fs::read_dir(&folder).map(|mut d| d.next().is_some()).unwrap_or(false);
    if folder.exists() && !folder_left {
        let _ = std::fs::remove_dir(&folder);
    }
    Json(json!({
        "chat_id": chat,
        "events": { "files": events.files, "bytes": events.bytes },
        "ledger": true,
        "saved_kv": kv,
        "folder_removed": !folder.exists(),
    }))
    .into_response()
}
