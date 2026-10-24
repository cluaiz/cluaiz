//! Chat memory API: a chat's event log, its Chat Map (cards), search over it, export, and
//! deleting a chat's history with everything derived from it.

use super::chat::recall;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use cluaiz_agents::memory::EventStore;
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
    match cluaiz_agents::memory::store_for(&chat).read(&chat) {
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
        Some(map) => {
            let graph = cluaiz_context::memory::build_graph(&map.cards, &map.episodes);
            Json(json!({ "chat_id": chat, "cards": map.cards, "episodes": map.episodes, "tasks": map.tasks, "graph": graph })).into_response()
        }
        None => error(StatusCode::INTERNAL_SERVER_ERROR, "could not read the chat's event log"),
    }
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub chat_id: String,
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    /// chat (default), or workspace / all to search the other chats (not with cross_chat: deny)
    #[serde(default)]
    pub scope: Option<String>,
}

/// POST /v1/recall/search: messages of a chat that fit a query (all of them are candidates)
pub async fn search(Json(request): Json<SearchRequest>) -> Response {
    if let Some(scope @ ("workspace" | "all")) = request.scope.as_deref() {
        if cluaiz_core::Settings::current().context_runtime.cross_chat == "deny" {
            return error(StatusCode::FORBIDDEN, "searching other chats is turned off (cross_chat: deny)");
        }
        let workspace = (scope == "workspace").then(super::notes::current_workspace);
        let text = recall::search_other_chats(&request.query, &request.chat_id, workspace.as_deref(), request.limit.unwrap_or(5).min(20));
        return Json(json!({ "chat_id": request.chat_id, "query": request.query, "scope": scope, "text": text })).into_response();
    }
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
    let log = match cluaiz_agents::memory::store_for(&chat).read(&chat) {
        Ok(log) => log,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let cards = recall::chat_map(&chat).map(|map| map.cards.clone()).unwrap_or_default();
    Json(json!({ "format": "cluaiz-chat-export", "version": 1, "chat_id": chat, "events": log.events, "cards": cards })).into_response()
}

/// DELETE /v1/chat/{session_id}/history: removes the chat's event log, ledger, saved KV, kept
/// context reports and Chat Map. Shared blobs stay (they are stored by content).
pub async fn delete_history(Path(chat): Path<String>) -> Response {
    let events = match cluaiz_agents::memory::store_for(&chat).delete_chat(&chat) {
        Ok(report) => report,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    cluaiz_agents::ChatLedger::delete(&chat);
    let kv = cluaiz_runtime::chat_memory::delete(&chat);
    recall::forget(&chat);
    cluaiz_agents::memory::external::forget_external(&chat);
    super::chat::authorize::revoke(&chat, None);
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

#[derive(Deserialize)]
pub struct GrantQuery {
    #[serde(default)]
    pub tool: Option<String>,
}

/// GET /v1/chat/{session_id}/grants: tools the user allowed for this chat, with minutes left
pub async fn get_grants(Path(chat): Path<String>) -> Json<Value> {
    let grants: Vec<Value> = super::chat::authorize::grants_of(&chat)
        .into_iter()
        .map(|(tool, minutes)| json!({ "tool": tool, "minutes_left": minutes }))
        .collect();
    Json(json!({ "chat_id": chat, "grants": grants }))
}

/// DELETE /v1/chat/{session_id}/grants[?tool=…]: asks again for this chat's tools (or one tool)
pub async fn revoke_grants(Path(chat): Path<String>, Query(query): Query<GrantQuery>) -> Json<Value> {
    let removed = super::chat::authorize::revoke(&chat, query.tool.as_deref());
    Json(json!({ "chat_id": chat, "removed": removed }))
}

#[derive(Deserialize)]
pub struct ImportChat {
    /// The events of `GET /v1/chat/{id}/export` (or of `memory_events` collected by the app)
    pub events: Vec<cluaiz_agents::memory::Event>,
}

/// POST /v1/chat/{session_id}/import: an app that keeps the chat itself (storage external) sends
/// it back, e.g. after an Engine restart. The chat is held in RAM only; nothing is written to disk.
pub async fn import_chat(Path(chat): Path<String>, Json(body): Json<ImportChat>) -> Response {
    if body.events.len() > 200_000 {
        return error(StatusCode::BAD_REQUEST, "at most 200000 events");
    }
    cluaiz_agents::memory::register_external(&chat);
    let count = body.events.len();
    cluaiz_agents::memory::external::mem_store().load(&chat, body.events);
    recall::forget(&chat);
    let cards = recall::chat_map(&chat).map_or(0, |map| map.cards.len());
    Json(json!({ "chat_id": chat, "storage": "external", "events": count, "cards": cards })).into_response()
}
