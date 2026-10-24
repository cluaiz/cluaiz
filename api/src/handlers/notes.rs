//! Durable notes API (`/v1/notes`): facts and preferences that outlive a chat. See
//! `cluaiz_agents::memory::notes`.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use cluaiz_agents::memory::notes::{Note, NoteStore};
use serde::Deserialize;
use serde_json::{json, Value};

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": { "message": message.into(), "type": "invalid_request_error" } }))).into_response()
}

/// The current workspace folder (the owner of workspace notes)
pub fn current_workspace() -> String {
    cluaiz_agents::ToolsEngine::workspace_dir().to_string_lossy().to_string()
}

#[derive(Deserialize)]
pub struct NotesQuery {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// Only notes that apply to this chat (with the current workspace)
    #[serde(default)]
    pub chat_id: Option<String>,
}

/// GET /v1/notes: notes, newest last (filters: scope, status, chat_id)
pub async fn list(Query(query): Query<NotesQuery>) -> Json<Value> {
    let workspace = current_workspace();
    let notes: Vec<Note> = NoteStore::shared()
        .all()
        .into_iter()
        .filter(|n| query.scope.as_deref().is_none_or(|s| n.scope == s))
        .filter(|n| query.status.as_deref().is_none_or(|s| n.status == s))
        .filter(|n| query.chat_id.as_deref().is_none_or(|c| n.applies_to(Some(c), Some(&workspace))))
        .collect();
    Json(json!({ "workspace": workspace, "notes": notes }))
}

#[derive(Deserialize)]
pub struct NewNote {
    pub text: String,
    /// chat, workspace (default) or global
    #[serde(default)]
    pub scope: Option<String>,
    /// The chat for scope chat; the current workspace for scope workspace when not given
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

/// POST /v1/notes: a note written by the user (confirmed at once). A live note of the same scope
/// on the same subject is superseded by it.
pub async fn create(Json(body): Json<NewNote>) -> Response {
    let text = body.text.trim();
    if text.is_empty() || text.len() > 2000 {
        return error(StatusCode::BAD_REQUEST, "text must be 1 to 2000 characters");
    }
    let scope = body.scope.as_deref().unwrap_or("workspace");
    let owner = match scope {
        "global" => None,
        "workspace" => Some(current_workspace()),
        "chat" => match &body.chat_id {
            Some(chat) => Some(chat.clone()),
            None => return error(StatusCode::BAD_REQUEST, "scope chat needs chat_id"),
        },
        _ => return error(StatusCode::BAD_REQUEST, "scope must be chat, workspace or global"),
    };
    match NoteStore::shared().add(text, scope, owner.as_deref(), "confirmed", body.pinned, "user", Vec::new()) {
        Ok((note, superseded)) => Json(json!({ "note": note, "superseded": superseded })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

fn change(id: &str, status: Option<&str>, pinned: Option<bool>) -> Response {
    match NoteStore::shared().update(id, status, pinned) {
        Ok(Some((note, superseded))) => Json(json!({ "note": note, "superseded": superseded })).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "no such note"),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// POST /v1/notes/{id}/confirm: a proposed note becomes confirmed (and supersedes a live note on
/// the same subject)
pub async fn confirm(Path(id): Path<String>) -> Response {
    change(&id, Some("confirmed"), None)
}

/// POST /v1/notes/{id}/invalidate: the note no longer holds (kept, with the time)
pub async fn invalidate(Path(id): Path<String>) -> Response {
    change(&id, Some("invalidated"), None)
}

#[derive(Deserialize)]
pub struct PinRequest {
    pub pinned: bool,
}

/// POST /v1/notes/{id}/pin: pinned notes go into the system prompt of new chats
pub async fn pin(Path(id): Path<String>, Json(body): Json<PinRequest>) -> Response {
    change(&id, None, Some(body.pinned))
}
