//! `memory.*`: a chat's event log, its Chat Map (cards), search over it, export and import, and
//! deleting a chat's history with everything derived from it.

use crate::cep::CallCtx;
use crate::handlers::chat::{authorize, context_plan, recall};
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

/// At most this many events in one import
const MAX_IMPORT_EVENTS: usize = 200_000;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChatId {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Events {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
    /// First seq to return (default 1)
    #[serde(default)]
    from_seq: Option<u64>,
    /// Most events to return (default 500, at most 5000)
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Search {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
    /// Words to look for (matched against the cards' titles, keywords and text)
    query: String,
    /// Most results (default 5; at most 50 in a chat, 20 across chats)
    #[serde(default)]
    limit: Option<usize>,
    /// `chat` (default), or `workspace` / `all` to search the other chats (not with cross_chat: deny)
    #[serde(default)]
    scope: Option<String>,
    /// Only messages from this time: `YYYY-MM-DD` (start of that day on the Engine machine's
    /// clock), `YYYY-MM-DDTHH:MM` (that clock) or RFC 3339
    #[serde(default)]
    from: Option<String>,
    /// Only messages up to this time: `YYYY-MM-DD` (to the end of that day), `YYYY-MM-DDTHH:MM` or
    /// RFC 3339
    #[serde(default)]
    to: Option<String>,
    /// Only messages of the last this many hours
    #[serde(default)]
    within_hours: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Item {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
    /// A reference from a search result, `m:<position>`
    #[serde(rename = "ref")]
    reference: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Import {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
    /// The events of `memory.export` (or the `memory_events` an app collected)
    events: Vec<Value>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Revoke {
    /// The chat: the same value as `session_id` in chat completions
    chat_id: String,
    /// Only this tool; all of the chat's grants when absent
    #[serde(default)]
    tool: Option<String>,
}

fn log_error(e: impl std::fmt::Display) -> CepError {
    CepError::unavailable("event_log_unreadable", format!("Could not read the chat's event log: {e}"))
}

fn chat_map(chat: &str) -> Result<std::sync::Arc<recall::ChatMap>, CepError> {
    recall::chat_map(chat).ok_or_else(|| log_error("no Chat Map"))
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "memory.events",
            Meta::new(Scope::Read, "A chat's event log, oldest first")
                .error(-32010, "event_log_unreadable", "The chat's event log could not be read")
                .example(
                json!({ "chat_id": "c1", "from_seq": 1, "limit": 2 }),
                json!({ "chat_id": "c1", "total": 40, "skipped_lines": 0, "next_from_seq": 3,
                        "events": [{ "seq": 1, "kind": "message", "data": { "role": "user", "text": "Hi" } }] }),
            ),
            |_: CallCtx, _: Call, p: Events| async move {
                let log = cluaiz_agents::memory::store_for(&p.chat_id).read(&p.chat_id).map_err(log_error)?;
                let from = p.from_seq.unwrap_or(1);
                let limit = p.limit.unwrap_or(500).min(5000);
                let total = log.events.len();
                let events: Vec<_> = log.events.into_iter().filter(|e| e.seq >= from).take(limit).collect();
                let next = events.last().map(|e| e.seq + 1).filter(|_| events.len() == limit);
                Ok(json!({ "chat_id": p.chat_id, "total": total, "skipped_lines": log.skipped_lines, "events": events, "next_from_seq": next }))
            },
        )
        .add(
            "memory.index",
            Meta::new(Scope::Read, "A chat's cards (one per visible message, no text), episodes, tasks and graph")
                .error(-32010, "event_log_unreadable", "The chat's event log could not be read")
                .example(
                json!({ "chat_id": "c1" }),
                json!({ "chat_id": "c1", "cards": [{ "pos": 0, "role": "user", "title": "Greeting" }], "episodes": [], "tasks": [], "graph": {} }),
            ),
            |_: CallCtx, _: Call, p: ChatId| async move {
                let map = chat_map(&p.chat_id)?;
                let graph = cluaiz_context::memory::build_graph(&map.cards, &map.episodes);
                Ok(json!({ "chat_id": p.chat_id, "cards": map.cards, "episodes": map.episodes, "tasks": map.tasks, "graph": graph }))
            },
        )
        .add(
            "memory.search",
            Meta::new(Scope::Read, "Messages of a chat that fit a query, or text from the other chats")
                .error(-32002, "forbidden", "`scope` workspace / all while `config` context `cross_chat` is deny (`data.needs: \"cross_chat\"`)")
                .error(-32602, "invalid_params", "`scope` is not chat, workspace or all, or `from` / `to` is not a time or `from` is not before `to`")
                .error(-32010, "event_log_unreadable", "The chat's event log could not be read")
                .example(
                json!({ "chat_id": "c1", "query": "deploy port" }),
                json!({ "chat_id": "c1", "query": "deploy port", "results": [{ "ref": "m:12", "score": 3.2, "why": ["port"], "card": {} }] }),
            ),
            |_: CallCtx, _: Call, p: Search| async move {
                let window = cluaiz_context::memory::window(p.from.as_deref(), p.to.as_deref(), p.within_hours, chrono::Utc::now())
                    .map_err(|e| CepError::invalid_params(e, Vec::new(), Some(json!({ "chat_id": p.chat_id, "query": p.query, "from": "2026-10-23", "to": "2026-10-23" }))))?;
                if let Some(scope @ ("workspace" | "all")) = p.scope.as_deref() {
                    if cluaiz_core::Settings::current().context_runtime.cross_chat == "deny" {
                        return Err(CepError::forbidden("cross_chat"));
                    }
                    let workspace = (scope == "workspace").then(recall::current_workspace);
                    let text = recall::search_other_chats(&p.query, &p.chat_id, workspace.as_deref(), window, p.limit.unwrap_or(5).min(20));
                    return Ok(json!({ "chat_id": p.chat_id, "query": p.query, "scope": scope, "text": text }));
                }
                if p.scope.as_deref().is_some_and(|s| s != "chat") {
                    return Err(CepError::invalid_params("`scope` is chat, workspace or all", Vec::new(), None));
                }
                let map = chat_map(&p.chat_id)?;
                let hits = cluaiz_context::memory::recall(&map.cards, &map.index, &p.query, &HashSet::new(), window, p.limit.unwrap_or(5).min(50));
                let results: Vec<Value> = hits
                    .iter()
                    .map(|hit| {
                        let card = &map.cards[hit.card];
                        json!({ "ref": format!("m:{}", card.pos), "score": (hit.score * 100.0).round() / 100.0, "why": hit.why, "card": card })
                    })
                    .collect();
                Ok(json!({ "chat_id": p.chat_id, "query": p.query, "results": results }))
            },
        )
        .add(
            "memory.item",
            Meta::new(Scope::Read, "The exact message behind a reference (m:<position>)")
                .error(-32602, "invalid_params", "`ref` does not look like m:<position>")
                .error(-32004, "message_not_found", "No visible message at that position")
                .error(-32010, "event_log_unreadable", "The chat's event log could not be read")
                .example(
                json!({ "chat_id": "c1", "ref": "m:12" }),
                json!({ "chat_id": "c1", "ref": "m:12", "card": {}, "text": "The deploy server listens on port 8443." }),
            ),
            |_: CallCtx, _: Call, p: Item| async move {
                let Some(pos) = p.reference.strip_prefix("m:").and_then(|n| n.parse::<u64>().ok()) else {
                    return Err(CepError::invalid_params("A reference looks like m:<position>", Vec::new(), Some(json!({ "chat_id": p.chat_id, "ref": "m:12" }))));
                };
                let map = chat_map(&p.chat_id)?;
                let i = map.cards.iter().position(|c| c.pos == pos).ok_or_else(|| CepError::not_found("message_not_found", "No visible message at that position"))?;
                Ok(json!({ "chat_id": p.chat_id, "ref": p.reference, "card": map.cards[i], "text": map.text(i) }))
            },
        )
        .add(
            "memory.export",
            Meta::new(Scope::Read, "A chat's events and cards, for an app's own store")
                .error(-32010, "event_log_unreadable", "The chat's event log could not be read")
                .example(
                json!({ "chat_id": "c1" }),
                json!({ "format": "cluaiz-chat-export", "version": 1, "chat_id": "c1", "events": [], "cards": [] }),
            ),
            |_: CallCtx, _: Call, p: ChatId| async move {
                let log = cluaiz_agents::memory::store_for(&p.chat_id).read(&p.chat_id).map_err(log_error)?;
                let cards = recall::chat_map(&p.chat_id).map(|map| map.cards.clone()).unwrap_or_default();
                Ok(json!({ "format": "cluaiz-chat-export", "version": 1, "chat_id": p.chat_id, "events": log.events, "cards": cards }))
            },
        )
        .add(
            "memory.import",
            Meta::new(Scope::Write, "Gives back a chat an app keeps itself (storage external), held in RAM only")
                .no_mcp()
                .error(-32006, "too_large", "More than 200 000 events")
                .error(-32602, "invalid_params", "An event does not have the event shape of `memory.export`")
                .example(
                json!({ "chat_id": "c1", "events": [] }),
                json!({ "chat_id": "c1", "storage": "external", "events": 0, "cards": 0 }),
            ),
            |_: CallCtx, _: Call, p: Import| async move {
                if p.events.len() > MAX_IMPORT_EVENTS {
                    return Err(CepError::too_large(MAX_IMPORT_EVENTS as u64, p.events.len() as u64));
                }
                let events: Vec<cluaiz_agents::memory::Event> = serde_json::from_value(Value::Array(p.events))
                    .map_err(|e| CepError::invalid_params(format!("Invalid event: {e}"), vec![json!({ "path": "events", "message": e.to_string() })], None))?;
                cluaiz_agents::memory::register_external(&p.chat_id);
                let count = events.len();
                cluaiz_agents::memory::external::mem_store().load(&p.chat_id, events);
                recall::forget(&p.chat_id);
                let cards = recall::chat_map(&p.chat_id).map_or(0, |map| map.cards.len());
                Ok(json!({ "chat_id": p.chat_id, "storage": "external", "events": count, "cards": cards }))
            },
        )
        .add(
            "memory.delete",
            Meta::new(Scope::Write, "Deletes a chat's event log, ledger, saved KV, context reports and Chat Map")
                .destructive()
                .error(-32010, "event_log_unreadable", "The chat's event log could not be removed")
                .example(
                json!({ "chat_id": "c1" }),
                json!({ "chat_id": "c1", "events": { "files": 1, "bytes": 2048 }, "ledger": true, "saved_kv": true, "folder_removed": true }),
            ),
            |_: CallCtx, _: Call, p: ChatId| async move {
                let chat = p.chat_id;
                let events = cluaiz_agents::memory::store_for(&chat).delete_chat(&chat).map_err(log_error)?;
                cluaiz_agents::ChatLedger::delete(&chat);
                let kv = cluaiz_runtime::chat_memory::delete(&chat);
                recall::forget(&chat);
                cluaiz_agents::memory::external::forget_external(&chat);
                authorize::forget(&chat);
                context_plan::forget_reports(&chat);
                // Shared blobs stay (they are stored by content); an empty chat folder goes
                let folder = cluaiz_agents::memory::memory_root().join("chats").join(cluaiz_agents::memory::chat_stem(&chat));
                let folder_left = folder.exists() && std::fs::read_dir(&folder).map(|mut d| d.next().is_some()).unwrap_or(false);
                if folder.exists() && !folder_left {
                    let _ = std::fs::remove_dir(&folder);
                }
                Ok(json!({
                    "chat_id": chat,
                    "events": { "files": events.files, "bytes": events.bytes },
                    "ledger": true,
                    "saved_kv": kv,
                    "folder_removed": !folder.exists(),
                }))
            },
        )
        .add(
            "memory.grants",
            Meta::new(Scope::Read, "Tools the user allowed for this chat, with minutes left").example(
                json!({ "chat_id": "c1" }),
                json!({ "chat_id": "c1", "grants": [{ "tool": "web-search", "minutes_left": 42 }] }),
            ),
            |_: CallCtx, _: Call, p: ChatId| async move {
                let grants: Vec<Value> =
                    authorize::grants_of(&p.chat_id).into_iter().map(|(tool, minutes)| json!({ "tool": tool, "minutes_left": minutes })).collect();
                Ok(json!({ "chat_id": p.chat_id, "grants": grants }))
            },
        )
        .add(
            "memory.revoke_grants",
            Meta::new(Scope::Write, "Asks again for this chat's tools (or one tool)").example(
                json!({ "chat_id": "c1", "tool": "web-search" }),
                json!({ "chat_id": "c1", "removed": 1 }),
            ),
            |_: CallCtx, _: Call, p: Revoke| async move {
                let removed = authorize::revoke(&p.chat_id, p.tool.as_deref());
                Ok(json!({ "chat_id": p.chat_id, "removed": removed }))
            },
        );
}
