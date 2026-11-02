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

/// A confirmed workspace note of a project the caller may write: it becomes a fact of the
/// project's `.agents/memory/` (`project_fact` in the result). Without one, the note stays as it is.
fn to_project(call: &Call, value: &mut Value, note: &Note) -> Result<(), CepError> {
    if !note.is_live() {
        return Ok(());
    }
    let Some(dir) = crate::handlers::chat::project::note_project(note) else { return Ok(()) };
    if call.check_path(&dir.join(".agents").join("memory")).is_err() {
        return Ok(());
    }
    let (saved, moved) = crate::handlers::chat::project::move_note(note, &dir)
        .map_err(|error| CepError::unavailable("memory_not_saved", format!("The note is confirmed, but its project fact was not saved: {error}")))?;
    value["note"] = json!(moved);
    value["project_fact"] = json!({ "name": saved.name, "path": crate::cep::roots::display(&saved.path), "replaced": saved.replaced });
    Ok(())
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
            |_: CallCtx, call: Call, p: Create| async move {
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
                let mut value = json!({ "note": note, "superseded": superseded });
                to_project(&call, &mut value, &note)?;
                Ok(value)
            },
        )
        .add(
            "notes.confirm",
            Meta::new(Scope::Write, "A proposed note becomes confirmed (and supersedes a live note on the same subject); a workspace note of a project becomes a fact of its `.agents/memory/`")
                .error(-32004, "note_not_found", "No note with this id")
                .error(-32010, "notes_unavailable", "The notes file could not be written")
                .error(-32010, "memory_not_saved", "The note is confirmed, but its project fact could not be written")
                .example(json!({ "id": "n7" }), json!({ "note": note, "superseded": [] })),
            |_: CallCtx, call: Call, p: Id| async move {
                let mut value = change(&p.id, Some("confirmed"), None)?;
                if let Ok(note) = serde_json::from_value::<Note>(value["note"].clone()) {
                    to_project(&call, &mut value, &note)?;
                }
                Ok(value)
            },
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
        )
        .add(
            "notes.move",
            Meta::new(Scope::Write, "A confirmed workspace note, saved before notes of a project became its `.agents/memory/` facts, becomes one now")
                .idempotent(false)
                .error(-32004, "note_not_found", "No note with this id")
                .error(-32602, "invalid_params", "The note is not a live workspace note of a project folder")
                .error(-32002, "forbidden", "The project's memory folder is outside the caller's roots")
                .error(-32010, "memory_not_saved", "The fact file could not be written")
                .example(
                    json!({ "id": "n7" }),
                    json!({ "note": { "id": "n7", "status": "moved", "moved_to": "D:/work/.agents/memory/prefers-tabs-over-spaces.md" }, "project_fact": { "name": "prefers-tabs-over-spaces", "path": "D:/work/.agents/memory/prefers-tabs-over-spaces.md", "replaced": [] } }),
                ),
            |_: CallCtx, call: Call, p: Id| async move {
                let note = NoteStore::shared().all().into_iter().find(|n| n.id == p.id).ok_or_else(|| CepError::not_found("note_not_found", format!("No note {}", p.id)))?;
                let dir = crate::handlers::chat::project::note_project(&note)
                    .filter(|_| note.is_live())
                    .ok_or_else(|| CepError::invalid_params("Only a live workspace note of a project folder can move", vec![json!({ "path": "id", "message": "not a project note" })], None))?;
                call.check_path(&dir.join(".agents").join("memory"))?;
                let (saved, moved) = crate::handlers::chat::project::move_note(&note, &dir)
                    .map_err(|error| CepError::unavailable("memory_not_saved", format!("The fact was not saved: {error}")))?;
                Ok(json!({ "note": moved, "project_fact": { "name": saved.name, "path": crate::cep::roots::display(&saved.path), "replaced": saved.replaced } }))
            },
        );
}
