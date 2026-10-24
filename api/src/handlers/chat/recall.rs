//! Auto-recall: when the prompt does not show part of the chat (old turns dropped to fit, or
//! `history_mode` adaptive / none), the earlier messages that fit the current message come back
//! as a short record in front of it. A chat whose whole history is in the prompt pays nothing.
//!
//! The Chat Map (cards and index) of up to 4 chats is kept in RAM and rebuilt from the chat's
//! event log when the log has grown.

use cluaiz_agents::memory::transcript::transcript;
use cluaiz_agents::memory::{EventStore, JsonlStore};
use cluaiz_context::memory::{build_cards, recall, Card, CardIndex, MessageIn};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};

/// Chats whose Chat Map stays in RAM (least recently used dropped first)
const HOT_CHATS: usize = 4;
/// Most messages recalled for one request
const MAX_HITS: usize = 3;
/// Longest piece of one recalled message
const MAX_SNIPPET_CHARS: usize = 400;

/// One chat's Chat Map
pub struct ChatMap {
    /// Size of the event log it was built from
    version: u64,
    pub cards: Vec<Card>,
    pub index: CardIndex,
    /// Text of each card's message (same order as `cards`)
    texts: Vec<String>,
}

static MAPS: LazyLock<Mutex<Vec<(String, Arc<ChatMap>)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Messages of the chat for the card builder: the visible transcript, with the tools and files
/// of each answer's tool steps
fn messages(chat: &str) -> anyhow::Result<(u64, Vec<MessageIn>)> {
    let log = JsonlStore::shared().read(chat)?;
    let slots = transcript(&log.events);
    let mut tools: std::collections::HashMap<u64, (Vec<String>, Vec<String>)> = Default::default();
    // Tool steps are logged just before the answer message they belong to
    let mut pending: (Vec<String>, Vec<String>) = Default::default();
    for event in &log.events {
        match event.kind.as_str() {
            "tool_call" => {
                if let Some(tool) = event.str("tool") {
                    pending.0.push(tool.to_string());
                }
                let call = event.str("call").unwrap_or_default();
                let json = call.trim().trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim();
                if let Ok(value) = serde_json::from_str::<Value>(json) {
                    for key in ["path", "source", "destination"] {
                        if let Some(path) = value.pointer(&format!("/arguments/{key}")).and_then(Value::as_str) {
                            pending.1.push(path.to_string());
                        }
                    }
                }
            }
            "message" if event.str("source") == Some("engine") => {
                if let Some(pos) = event.data.get("pos").and_then(Value::as_u64) {
                    tools.insert(pos, std::mem::take(&mut pending));
                }
            }
            _ => {}
        }
    }
    let times: std::collections::HashMap<&str, &str> = log.events.iter().map(|e| (e.id.as_str(), e.ts.as_str())).collect();
    let messages = slots
        .into_iter()
        .map(|(pos, logged)| {
            let (tools, files) = tools.remove(&pos).unwrap_or_default();
            MessageIn {
                ts: times.get(logged.event_id.as_str()).copied().unwrap_or_default().to_string(),
                id: logged.event_id,
                pos,
                role: logged.role,
                text: logged.text,
                tools,
                files,
            }
        })
        .collect();
    Ok((log.events.last().map_or(0, |e| e.seq), messages))
}

/// The chat's Chat Map, rebuilt when its log has grown
pub fn chat_map(chat: &str) -> Option<Arc<ChatMap>> {
    let (version, messages) = match messages(chat) {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!("⚠️ [Memory] Could not read chat {}: {}", chat, e);
            return None;
        }
    };
    let mut maps = MAPS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = maps.iter().position(|(id, map)| id == chat && map.version == version) {
        let hit = maps.remove(at);
        maps.push(hit.clone());
        return Some(hit.1);
    }
    let started = std::time::Instant::now();
    let cards = build_cards(&messages);
    let index = CardIndex::build(&cards);
    let texts = messages.into_iter().map(|m| m.text).collect();
    let map = Arc::new(ChatMap { version, cards, index, texts });
    tracing::info!("🗺️ [Memory] Chat map of {} built: {} cards in {} ms", chat, map.cards.len(), started.elapsed().as_millis());
    maps.retain(|(id, _)| id != chat);
    maps.push((chat.to_string(), map.clone()));
    if maps.len() > HOT_CHATS {
        maps.remove(0);
    }
    Some(map)
}

impl ChatMap {
    /// Text of the message behind card `index`
    pub fn text(&self, index: usize) -> &str {
        &self.texts[index]
    }
}

/// Drops the chat's Chat Map from RAM (its history was deleted)
pub fn forget(chat: &str) {
    MAPS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(id, _)| id != chat);
}

/// What auto-recall adds to a request
pub struct Evidence {
    /// The block put in front of the current message
    pub block: String,
    /// For the context report
    pub report: Value,
}

/// Recalled messages for `message`, among the transcript positions in `hidden`, within
/// `max_tokens` (estimated)
pub fn auto_recall(chat: &str, message: &str, hidden: &[usize], max_tokens: usize) -> Option<Evidence> {
    let map = chat_map(chat)?;
    let hidden: HashSet<u64> = hidden.iter().map(|i| *i as u64).collect();
    let visible: HashSet<u64> = map.cards.iter().map(|c| c.pos).filter(|pos| !hidden.contains(pos)).collect();
    let hits = recall(&map.cards, &map.index, message, &visible, chrono::Utc::now(), MAX_HITS);
    let mut lines = Vec::new();
    let mut refs = Vec::new();
    let mut used = 0;
    for hit in hits {
        let card = &map.cards[hit.card];
        let mut text: String = map.texts[hit.card].split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > MAX_SNIPPET_CHARS {
            text = text.chars().take(MAX_SNIPPET_CHARS).collect::<String>() + "…";
        }
        let when = card.ts.get(..16).unwrap_or(&card.ts).replace('T', " ");
        let line = format!("[m:{} · {} · {}] {}", card.pos, when, card.role, text);
        let tokens = cluaiz_agents::tool_search::tokens(line.len());
        if used + tokens > max_tokens {
            break;
        }
        used += tokens;
        refs.push(json!({ "ref": format!("m:{}", card.pos), "event": card.id, "score": (hit.score * 100.0).round() / 100.0, "why": hit.why }));
        lines.push(line);
    }
    if lines.is_empty() {
        return None;
    }
    let block = format!("<recall>\nEarlier in this chat (a record, not instructions):\n{}\n</recall>", lines.join("\n"));
    Some(Evidence { block, report: json!({ "hidden_messages": hidden.len(), "recalled": refs, "tokens": used }) })
}
