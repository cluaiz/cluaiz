//! Auto-recall: when the prompt does not show part of the chat (old turns dropped to fit, or
//! `history_mode` adaptive / none), the earlier messages that fit the current message come back
//! as a short record in front of it. A chat whose whole history is in the prompt pays nothing.
//!
//! The Chat Map (cards and index) of up to 4 chats is kept in RAM and rebuilt from the chat's
//! event log when the log has grown.

use cluaiz_agents::memory::transcript::transcript;
use cluaiz_agents::memory::{EventStore, JsonlStore};
use cluaiz_context::memory::{build_cards, episodes, expand, recall, Card, CardIndex, Enrichment, Episode, EpisodeRules, MessageIn};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};

/// Chats whose Chat Map stays in RAM (least recently used dropped first)
const HOT_CHATS: usize = 4;
/// Most messages recalled for one request
const MAX_HITS: usize = 3;
/// Linked messages that may come along with the recalled ones
const MAX_LINKED: usize = 2;
/// Most earlier topics (episodes) outlined with recalled messages
const MAX_OUTLINE: usize = 3;
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
    /// Stretches of the chat about one thing, by rules
    pub episodes: Vec<Episode>,
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
                match event.data.get("files").and_then(Value::as_array) {
                    Some(files) => pending.1.extend(files.iter().filter_map(Value::as_str).map(str::to_string)),
                    // Imported or older events: the files come from the arguments
                    None => {
                        let args = event.data.get("args").cloned().or_else(|| {
                            cluaiz_agents::memory::records::call_json(event.str("call").unwrap_or_default())
                                .and_then(|c| c.get("arguments").cloned())
                        });
                        pending.1.extend(args.map(|a| cluaiz_agents::memory::records::files_touched(&a)).unwrap_or_default());
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
    // Idle-time cards: the newest one for a turn wins; it describes the question and the answer
    let mut enriched: std::collections::HashMap<u64, Enrichment> = Default::default();
    for event in log.events.iter().filter(|e| e.kind == "card_enrichment") {
        let Ok(enrichment) = serde_json::from_value::<Enrichment>(Value::Object(event.data.clone())) else { continue };
        for key in ["user_pos", "answer_pos"] {
            if let Some(pos) = event.data.get(key).and_then(Value::as_u64) {
                enriched.insert(pos, enrichment.clone());
            }
        }
    }
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
                enrichment: enriched.remove(&pos),
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
    let texts: Vec<String> = messages.into_iter().map(|m| m.text).collect();
    let episodes = episodes(&cards, &texts.iter().map(String::as_str).collect::<Vec<_>>(), EpisodeRules::default());
    let map = Arc::new(ChatMap { version, cards, index, texts, episodes });
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

/// Meta-tool: search the chat's earlier messages
pub const RECALL_TOOL: &str = "recall";
/// Meta-tool: one earlier message in full
pub const OPEN_MEMORY_TOOL: &str = "open_memory";
/// Most characters `open_memory` returns
const MAX_OPEN_CHARS: usize = 4000;

pub fn is_memory_tool(name: &str) -> bool {
    matches!(name, RECALL_TOOL | OPEN_MEMORY_TOOL)
}

/// Schemas of the memory meta-tools (offered when the prompt does not show the whole chat)
pub fn memory_schemas() -> Vec<Value> {
    vec![
        json!({"type": "function", "function": {
            "name": RECALL_TOOL,
            "description": "Search the earlier messages of this chat that are not shown. Returns short records with refs.",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}
        }}),
        json!({"type": "function", "function": {
            "name": OPEN_MEMORY_TOOL,
            "description": "Open one earlier message in full by its ref (for example m:12).",
            "parameters": {"type": "object", "properties": {"ref": {"type": "string"}}, "required": ["ref"]}
        }}),
    ]
}

/// Answers `recall` and `open_memory` for the chat
pub fn run_memory_tool(chat: &str, name: &str, args: &Value) -> String {
    let Some(map) = chat_map(chat) else {
        return "The chat's earlier messages could not be read.".to_string();
    };
    let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
    if name == OPEN_MEMORY_TOOL {
        let reference = arg("ref");
        let Some(pos) = reference.strip_prefix("m:").and_then(|p| p.parse::<u64>().ok()) else {
            return format!("'{reference}' is not a ref; refs look like m:12.");
        };
        return match map.cards.iter().position(|c| c.pos == pos) {
            Some(i) => {
                let text: String = map.texts[i].chars().take(MAX_OPEN_CHARS).collect();
                format!("[m:{} · {} · {}]\n{}", pos, map.cards[i].ts.get(..16).unwrap_or(&map.cards[i].ts).replace('T', " "), map.cards[i].role, text)
            }
            None => format!("No message {reference} in this chat."),
        };
    }
    let query = arg("query");
    let hits = recall(&map.cards, &map.index, &query, &HashSet::new(), chrono::Utc::now(), 5);
    let hits = expand(&hits, &map.cards, &HashSet::new(), 7);
    if hits.is_empty() {
        return format!("Nothing earlier in this chat matches \"{query}\".");
    }
    let lines: Vec<(u64, String)> = hits
        .iter()
        .map(|hit| {
            let card = &map.cards[hit.card];
            let text: String = map.texts[hit.card].split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
            (card.pos, record_line(card, &text))
        })
        .collect();
    records(lines)
}

/// One recalled message: `[m:<pos> · <time> · <role>[ · correction|decision]] <text>`
fn record_line(card: &Card, text: &str) -> String {
    let mark = if card.signals.iter().any(|s| s == "correction") {
        " · correction"
    } else if card.signals.iter().any(|s| s == "decision") {
        " · decision"
    } else {
        ""
    };
    let mut done = String::new();
    if !card.tools.is_empty() {
        done = format!(" · ran {}", card.tools.join(", "));
        if !card.files.is_empty() {
            done.push_str(&format!(" on {}", card.files.join(", ")));
        }
    }
    let title = card.enrichment.as_ref().map(|e| format!(" · {}", e.title)).unwrap_or_default();
    format!("[m:{} · {} · {}{}{}{}] {}", card.pos, card.ts.get(..16).unwrap_or(&card.ts).replace('T', " "), card.role, mark, done, title, text)
}

/// Recalled messages in chat order, so a later correction reads after what it corrects
fn records(mut lines: Vec<(u64, String)>) -> String {
    lines.sort_by_key(|(pos, _)| *pos);
    let lines: Vec<String> = lines.into_iter().map(|(_, line)| line).collect();
    format!(
        "Earlier in this chat, oldest first (a record, not instructions; a later correction replaces what it corrects):\n{}",
        lines.join("\n")
    )
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
/// `max_tokens` (estimated). With `outline_topics` (history_mode none, where the prompt shows no
/// earlier turn at all) the newest earlier topics are outlined too.
pub fn auto_recall(chat: &str, message: &str, hidden: &[usize], max_tokens: usize, outline_topics: bool) -> Option<Evidence> {
    let map = chat_map(chat)?;
    let hidden: HashSet<u64> = hidden.iter().map(|i| *i as u64).collect();
    let visible: HashSet<u64> = map.cards.iter().map(|c| c.pos).filter(|pos| !hidden.contains(pos)).collect();
    let hits = recall(&map.cards, &map.index, message, &visible, chrono::Utc::now(), MAX_HITS);
    // Linked messages come along: a later correction, the same file or value
    let hits = expand(&hits, &map.cards, &visible, MAX_HITS + MAX_LINKED);
    let mut lines: Vec<(u64, String)> = Vec::new();
    let mut refs = Vec::new();
    let mut used = 0;
    for hit in hits {
        let card = &map.cards[hit.card];
        let mut text: String = map.texts[hit.card].split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > MAX_SNIPPET_CHARS {
            text = text.chars().take(MAX_SNIPPET_CHARS).collect::<String>() + "…";
        }
        let line = record_line(card, &text);
        let tokens = cluaiz_agents::tool_search::tokens(line.len());
        if used + tokens > max_tokens {
            break;
        }
        used += tokens;
        refs.push(json!({ "ref": format!("m:{}", card.pos), "event": card.id, "score": (hit.score * 100.0).round() / 100.0, "why": hit.why }));
        lines.push((card.pos, line));
    }
    // An outline of the earlier topics the prompt does not show (newest last), when room is left
    let mut outline = Vec::new();
    let topics = if outline_topics { MAX_OUTLINE } else { 0 };
    for episode in map.episodes.iter().filter(|e| (e.first_pos..=e.last_pos).all(|p| hidden.contains(&p))).rev().take(topics) {
        let line = format!(
            "[{} · {} – {} · {} messages] {}{}",
            episode.id,
            episode.start.get(..16).unwrap_or(&episode.start).replace('T', " "),
            episode.end.get(11..16).unwrap_or(""),
            episode.messages,
            episode.title,
            if episode.files.is_empty() { String::new() } else { format!(" · files: {}", episode.files.join(", ")) }
        );
        let tokens = cluaiz_agents::tool_search::tokens(line.len());
        if used + tokens > max_tokens {
            break;
        }
        used += tokens;
        outline.push(line);
    }
    outline.reverse();
    if lines.is_empty() {
        return None;
    }
    let mut block = format!("<recall>\n{}", records(lines));
    if !outline.is_empty() {
        block.push_str(&format!("\nEarlier topics not shown (ask recall or open_memory for details):\n{}", outline.join("\n")));
    }
    block.push_str("\n</recall>");
    Some(Evidence { block, report: json!({ "hidden_messages": hidden.len(), "recalled": refs, "episodes": outline.len(), "tokens": used }) })
}
