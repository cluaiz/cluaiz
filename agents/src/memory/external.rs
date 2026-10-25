//! External storage mode (`storage: "external"`): the app keeps the chat, the Engine keeps it in
//! RAM only. Nothing of such a chat is written to disk (no event log, ledger or saved KV); the app
//! gets the events with each answer and sends the chat back with `POST /v1/chat/{id}/import`
//! after a restart.

use super::events::{Event, NewEvent, EVENT_VERSION};
use super::store::{DeleteReport, EventStore, JsonlStore, ReadLog};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

/// Events of chats kept in RAM
#[derive(Default)]
pub struct MemStore {
    chats: Mutex<HashMap<String, Vec<Event>>>,
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    /// The last seq the app was given, per chat
    delivered: Mutex<HashMap<String, u64>>,
}

impl MemStore {
    /// Replaces a chat's events (import); the app has all of them
    pub fn load(&self, chat: &str, mut events: Vec<Event>) {
        events.sort_by_key(|e| e.seq);
        for event in &mut events {
            event.chat = chat.to_string();
        }
        let last = events.last().map_or(0, |e| e.seq);
        self.chats.lock().unwrap_or_else(|e| e.into_inner()).insert(chat.to_string(), events);
        self.delivered.lock().unwrap_or_else(|e| e.into_inner()).insert(chat.to_string(), last);
    }

    /// Events the app has not been given yet, oldest first: what a request and its answer added,
    /// and what was added after the last answer (an idle-time card, a note). They count as given.
    pub fn undelivered(&self, chat: &str) -> Vec<Event> {
        let chats = self.chats.lock().unwrap_or_else(|e| e.into_inner());
        let mut delivered = self.delivered.lock().unwrap_or_else(|e| e.into_inner());
        let after = delivered.get(chat).copied().unwrap_or(0);
        let events: Vec<Event> = chats.get(chat).map(|log| log.iter().filter(|e| e.seq > after).cloned().collect()).unwrap_or_default();
        if let Some(last) = events.last() {
            delivered.insert(chat.to_string(), last.seq);
        }
        events
    }
}

impl EventStore for MemStore {
    fn append(&self, chat: &str, events: Vec<NewEvent>) -> Result<Vec<Event>> {
        let mut chats = self.chats.lock().unwrap_or_else(|e| e.into_inner());
        let log = chats.entry(chat.to_string()).or_default();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut written = Vec::new();
        for mut new in events {
            for reserved in ["v", "id", "chat", "seq", "ts", "kind"] {
                new.data.remove(reserved);
            }
            // Same contract as the disk store: large string fields become blob references
            for value in new.data.values_mut() {
                if let serde_json::Value::String(text) = value {
                    if text.len() > super::store::BLOB_BYTES {
                        let reference = self.put_blob(text.as_bytes())?;
                        *value = serde_json::json!({ "blob": reference, "size": text.len() });
                    }
                }
            }
            let event = Event {
                v: EVENT_VERSION,
                id: format!("ev_{}", uuid::Uuid::new_v4().simple()),
                chat: chat.to_string(),
                seq: log.last().map_or(1, |e| e.seq + 1),
                ts: new.ts.unwrap_or_else(|| now.clone()),
                kind: new.kind,
                data: new.data,
            };
            log.push(event.clone());
            written.push(event);
        }
        Ok(written)
    }

    fn sync(&self, _chat: &str) -> Result<()> {
        Ok(())
    }

    fn read(&self, chat: &str) -> Result<ReadLog> {
        let events = self.chats.lock().unwrap_or_else(|e| e.into_inner()).get(chat).cloned().unwrap_or_default();
        Ok(ReadLog { events, skipped_lines: 0 })
    }

    fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        use sha2::{Digest, Sha256};
        let reference = format!("sha256:{:x}", Sha256::digest(bytes));
        self.blobs.lock().unwrap_or_else(|e| e.into_inner()).insert(reference.clone(), bytes.to_vec());
        Ok(reference)
    }

    fn get_blob(&self, reference: &str) -> Result<Vec<u8>> {
        self.blobs.lock().unwrap_or_else(|e| e.into_inner()).get(reference).cloned().ok_or_else(|| anyhow::anyhow!("no such blob"))
    }

    fn delete_chat(&self, chat: &str) -> Result<DeleteReport> {
        // One in-RAM log; bytes as its JSON size
        self.delivered.lock().unwrap_or_else(|e| e.into_inner()).remove(chat);
        let removed = self.chats.lock().unwrap_or_else(|e| e.into_inner()).remove(chat);
        Ok(match removed {
            Some(events) => DeleteReport { files: 1, bytes: serde_json::to_vec(&events).map_or(0, |b| b.len() as u64) },
            None => DeleteReport::default(),
        })
    }
}

static EXTERNAL: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
static MEM_STORE: LazyLock<MemStore> = LazyLock::new(MemStore::default);

/// Marks a chat as kept by the app (RAM only from now on, until the Engine restarts)
pub fn register_external(chat: &str) {
    EXTERNAL.lock().unwrap_or_else(|e| e.into_inner()).insert(chat.to_string());
}

/// Forgets that a chat is external (its history was deleted)
pub fn forget_external(chat: &str) {
    EXTERNAL.lock().unwrap_or_else(|e| e.into_inner()).remove(chat);
}

pub fn is_external(chat: &str) -> bool {
    EXTERNAL.lock().unwrap_or_else(|e| e.into_inner()).contains(chat)
}

/// The in-RAM store of external chats
pub fn mem_store() -> &'static MemStore {
    &MEM_STORE
}

/// Where a chat's events live: RAM for external chats, the JSONL log otherwise
pub fn store_for(chat: &str) -> &'static dyn EventStore {
    if is_external(chat) {
        &*MEM_STORE
    } else {
        JsonlStore::shared()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_store_conformance() {
        crate::memory::store::tests::conformance(&MemStore::default());
    }

    #[test]
    fn test_undelivered_events_go_out_once() {
        let store = MemStore::default();
        let message = |text: &str| NewEvent::new("message", serde_json::json!({ "role": "user", "text": text }));
        store.append("c", vec![message("a"), message("b")]).unwrap();
        assert_eq!(store.undelivered("c").len(), 2);
        assert!(store.undelivered("c").is_empty());
        // Added after the answer (an idle-time card): it goes out with the next one
        store.append("c", vec![NewEvent::new("card_enrichment", serde_json::json!({ "title": "t" }))]).unwrap();
        let next = store.undelivered("c");
        assert_eq!((next.len(), next[0].kind.as_str()), (1, "card_enrichment"));
        // An import counts as given
        let events = store.read("c").unwrap().events;
        store.load("c", events);
        assert!(store.undelivered("c").is_empty());
    }

    #[test]
    fn test_external_chats_use_ram() {
        register_external("ext-test");
        let store = store_for("ext-test");
        store.append("ext-test", vec![NewEvent::new("message", serde_json::json!({ "role": "user", "text": "hi" }))]).unwrap();
        assert_eq!(mem_store().read("ext-test").unwrap().events.len(), 1);
        forget_external("ext-test");
        assert!(!is_external("ext-test"));
    }
}
