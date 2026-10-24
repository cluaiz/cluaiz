//! Chat memory on disk: each chat's event log (what was said and done, in order), from which the
//! Chat Map (cards, indexes, recall) is built. JSON lines and plain files only; no database.
//!
//! ```text
//! <CLUAIZ_HOME>/memory/
//!   chats/<chat>/events.jsonl            current segment (append-only)
//!   chats/<chat>/events-0001.jsonl.zst   closed segments
//!   chats/<chat>/ledger.json             what the Engine added to each turn (lifecycle::ledger)
//!   blobs/<sha256[0..2]>/<sha256>        large payloads (tool outputs over 8 KB)
//! ```

pub mod events;
pub mod records;
pub mod store;
pub mod transcript;

pub use events::{Event, NewEvent};
pub use store::{DeleteReport, EventStore, JsonlStore, ReadLog};

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// `<CLUAIZ_HOME>/memory`
pub fn memory_root() -> PathBuf {
    cluaiz_core::EnvironmentManager::current().global_dir.join("memory")
}

/// Folder name for a chat id: the id itself when it is a plain name, else its hash
pub fn chat_stem(chat_id: &str) -> String {
    let plain = !chat_id.is_empty()
        && chat_id.len() <= 64
        && chat_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        return chat_id.to_owned();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    chat_id.hash(&mut hasher);
    format!("chat-{:016x}", hasher.finish())
}
