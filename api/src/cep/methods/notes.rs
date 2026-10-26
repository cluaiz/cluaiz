//! `notes.*`: durable notes, facts and preferences that outlive a chat. See
//! `cluaiz_agents::memory::notes`.

use crate::cep::CallCtx;
use crate::handlers::chat::recall::current_workspace;
use cluaiz_agents::memory::notes::{Note, NoteStore};
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct List {
    /// chat, workspace or global
    #[serde(default)]
    scope: Option<String>,
    /// proposed, confirmed, superseded or invalidated
    #[serde(default)]
    status: Option<String>,
    /// Only notes that apply to this chat (with the current workspace)
    #[serde(default)]
    chat_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Create {
    /// 1 to 2000 characters
    text: String,
    /// chat, workspace (default) or global
    #[serde(default)]
    scope: Option<String>,
    /// The chat, for scope chat
    #[serde(default)]
    chat_id: Option<String>,
    /// Pinned notes go into the system prompt of new chats
    #[serde(default)]
    pinned: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Id {
    /// The note's `id` (from `notes.list`)
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Pin {
    /// The note's `id` (from `notes.list`)
    id: String,
    /// true: goes into the system prompt of new chats; false: does not
    pinned: bool,
}

fn store_error(e: impl std::fmt::Display) -> CepError {
    CepError::unavailable("notes_unavailable", format!("Notes could not be saved: {e}"))
}

fn change(id: &str, status: Option<&str>, pinned: Option<bool>) -> Result<Value, CepError> {
    match NoteStore::shared().update(id, status, pinned).map_err(store_error)? {
        Some((note, superseded)) => Ok(json!({ "note": note, "superseded": superseded })),
        None => Err(CepError::not_found("note_not_found", format!("No note {id}"))),
    }
}

pub fn register(registry: &mut Registry<CallCtx>) {
    let note = json!({ "id": "n7", "text": "Prefers tabs over spaces", "scope": "workspace", "status": "confirmed", "pinned": false });
    registry
        .add(
            "notes.list",
            Meta::new(Scope::Read, "Notes, newest last").example(json!({ "status": "confirmed" }), json!({ "workspace": "D:/work", "notes": [note] })),
            |_: CallCtx, _: Call, p: List| async move {
                let workspace = current_workspace();
                let notes: Vec<Note> = NoteStore::shared()
                    .all()
                    .into_iter()
                    .filter(|n| p.scope.as_deref().is_none_or(|s| n.scope == s))
                    .filter(|n| p.status.as_deref().is_none_or(|s| n.status == s))
                    .filter(|n| p.chat_id.as_deref().is_none_or(|c| n.applies_to(Some(c), Some(&workspace))))
                    .collect();
                Ok(json!({ "workspace": workspace, "notes": notes }))
            },
        )
        .add(
            "notes.create",
            Meta::new(Scope::Write, "A note written by the user, confirmed at once; it supersedes a live note on the same subject")
                .idempotent(false)
                .error(-32602, "invalid_params", "`text` is empty or over 2000 characters, `scope` is unknown, or scope chat has no `chat_id`")
                .error(-32010, "notes_unavailable", "The notes file could not be written")
                .example(json!({ "text": "Prefers tabs over spaces", "scope": "workspace" }), json!({ "note": note, "superseded": [] })),
            |_: CallCtx, _: Call, p: Create| async move {
                let text = p.text.trim();
                if text.is_empty() || text.len() > 2000 {
                    return Err(CepError::invalid_params("`text` must be 1 to 2000 characters", vec![json!({ "path": "text", "message": "length" })], None));
                }
                let scope = p.scope.as_deref().unwrap_or("workspace");
                let owner = match scope {
                    "global" => None,
                    "workspace" => Some(current_workspace()),
                    "chat" => Some(p.chat_id.clone().ok_or_else(|| CepError::invalid_params("scope chat needs `chat_id`", Vec::new(), None))?),
                    _ => return Err(CepError::invalid_params("`scope` is chat, workspace or global", Vec::new(), None)),
                };
                let (note, superseded) =
                    NoteStore::shared().add(text, scope, owner.as_deref(), "confirmed", p.pinned, "user", Vec::new()).map_err(store_error)?;
                Ok(json!({ "note": note, "superseded": superseded }))
            },
        )
        .add(
            "notes.confirm",
            Meta::new(Scope::Write, "A proposed note becomes confirmed (and supersedes a live note on the same subject)")
                .error(-32004, "note_not_found", "No note with this id")
                .error(-32010, "notes_unavailable", "The notes file could not be written")
                .example(json!({ "id": "n7" }), json!({ "note": note, "superseded": [] })),
            |_: CallCtx, _: Call, p: Id| async move { change(&p.id, Some("confirmed"), None) },
        )
        .add(
            "notes.invalidate",
            Meta::new(Scope::Write, "The note no longer holds (it is kept, with the time)")
                .error(-32004, "note_not_found", "No note with this id")
                .error(-32010, "notes_unavailable", "The notes file could not be written")
                .example(json!({ "id": "n7" }), json!({ "note": note, "superseded": [] })),
            |_: CallCtx, _: Call, p: Id| async move { change(&p.id, Some("invalidated"), None) },
        )
        .add(
            "notes.pin",
            Meta::new(Scope::Write, "Pins or unpins a note; pinned notes go into the system prompt of new chats")
                .error(-32004, "note_not_found", "No note with this id")
                .error(-32010, "notes_unavailable", "The notes file could not be written")
                .example(json!({ "id": "n7", "pinned": true }), json!({ "note": note, "superseded": [] })),
            |_: CallCtx, _: Call, p: Pin| async move { change(&p.id, None, Some(p.pinned)) },
        );
}
