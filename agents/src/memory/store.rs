//! Where a chat's events live. `EventStore` is the backend contract; `JsonlStore` is the built-in
//! backend (JSON lines per chat, zstd for closed segments, content-addressed blobs).
//!
//! Durability: every append is written and flushed when it happens; `sync` (called at the end of a
//! turn) asks the OS to put it on disk. A line cut short by a crash is skipped and reported when
//! the log is read, so the rest of the chat stays readable.

use super::events::{Event, NewEvent, EVENT_VERSION};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

/// A segment is closed (and compressed) past this size
pub const SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
/// String fields longer than this go into blobs, referenced from the event
pub const BLOB_BYTES: usize = 8 * 1024;

/// Chats whose parsed log stays in RAM (least recently read dropped first)
const HOT_LOGS: usize = 4;

/// A chat's events as read, with the lines that could not be read
#[derive(Debug, Default, Clone)]
pub struct ReadLog {
    pub events: Vec<Event>,
    /// Lines that were not valid events (a crash during an append leaves a cut last line)
    pub skipped_lines: usize,
}

/// What deleting a chat removed
#[derive(Debug, Default, Serialize)]
pub struct DeleteReport {
    pub files: usize,
    pub bytes: u64,
}

/// The backend contract. Every backend must pass `tests::conformance`.
pub trait EventStore: Send + Sync {
    /// Appends events to the chat's log, giving each its id, seq and time
    fn append(&self, chat: &str, events: Vec<NewEvent>) -> Result<Vec<Event>>;
    /// Makes the chat's appended events durable (end of a turn)
    fn sync(&self, chat: &str) -> Result<()>;
    /// All events of the chat, oldest first
    fn read(&self, chat: &str) -> Result<ReadLog>;
    /// The same, shared: a backend may keep the logs of hot chats in RAM, so a request does not
    /// parse the whole log again
    fn read_shared(&self, chat: &str) -> Result<Arc<ReadLog>> {
        Ok(Arc::new(self.read(chat)?))
    }
    /// Stores bytes by content; returns the reference (`sha256:<hex>`)
    fn put_blob(&self, bytes: &[u8]) -> Result<String>;
    fn get_blob(&self, reference: &str) -> Result<Vec<u8>>;
    /// Removes the chat's log (blobs are shared by content and stay)
    fn delete_chat(&self, chat: &str) -> Result<DeleteReport>;
}

/// Per-chat state: next seq and the size of the open segment
struct ChatState {
    next_seq: u64,
    open_bytes: u64,
}

/// The built-in backend: `<root>/chats/<chat>/events*.jsonl[.zst]`, `<root>/blobs/`
pub struct JsonlStore {
    root: PathBuf,
    /// Size at which the open segment is closed (`SEGMENT_BYTES`)
    segment_bytes: u64,
    chats: Mutex<HashMap<String, Arc<Mutex<Option<ChatState>>>>>,
    /// Parsed logs of hot chats with the files they were read from (closed segments, open
    /// segment size): appends extend them, and a file changed from outside is read again
    hot: Mutex<Vec<(String, (usize, u64), Arc<ReadLog>)>>,
}

static DEFAULT_STORE: LazyLock<JsonlStore> = LazyLock::new(|| JsonlStore::new(super::memory_root()));

impl JsonlStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root, segment_bytes: SEGMENT_BYTES, chats: Mutex::new(HashMap::new()), hot: Mutex::new(Vec::new()) }
    }

    /// The store under `<CLUAIZ_HOME>/memory`
    pub fn shared() -> &'static JsonlStore {
        &DEFAULT_STORE
    }

    fn chat_dir(&self, chat: &str) -> PathBuf {
        self.root.join("chats").join(super::chat_stem(chat))
    }

    fn open_segment(&self, chat: &str) -> PathBuf {
        self.chat_dir(chat).join("events.jsonl")
    }

    /// Closed segments, oldest first
    fn closed_segments(&self, chat: &str) -> Vec<PathBuf> {
        let mut segments: Vec<PathBuf> = std::fs::read_dir(self.chat_dir(chat))
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("events-") && n.ends_with(".jsonl.zst")))
                    .collect()
            })
            .unwrap_or_default();
        segments.sort();
        segments
    }

    /// What the chat's log files look like now: closed segments and the open segment's size
    fn stamp(&self, chat: &str) -> (usize, u64) {
        (self.closed_segments(chat).len(), std::fs::metadata(self.open_segment(chat)).map(|m| m.len()).unwrap_or(0))
    }

    fn lock(&self, chat: &str) -> Arc<Mutex<Option<ChatState>>> {
        self.chats.lock().unwrap_or_else(|e| e.into_inner()).entry(chat.to_string()).or_default().clone()
    }

    /// Large string fields go into blobs: `{"blob": "sha256:…", "size": n}`
    fn move_large_fields(&self, event: &mut NewEvent) -> Result<()> {
        for value in event.data.values_mut() {
            if let Value::String(text) = value {
                if text.len() > BLOB_BYTES {
                    let reference = self.put_blob(text.as_bytes())?;
                    *value = serde_json::json!({ "blob": reference, "size": text.len() });
                }
            }
        }
        Ok(())
    }

    fn blob_path(&self, hex: &str) -> PathBuf {
        self.root.join("blobs").join(&hex[..2]).join(hex)
    }

    /// Closes the open segment: compressed to the next `events-NNNN.jsonl.zst`
    fn rotate(&self, chat: &str) -> Result<()> {
        let open = self.open_segment(chat);
        let next = self.closed_segments(chat).len() + 1;
        let closed = self.chat_dir(chat).join(format!("events-{next:04}.jsonl.zst"));
        let input = File::open(&open)?;
        let output = File::create(&closed)?;
        let mut encoder = zstd::stream::write::Encoder::new(output, 3)?;
        std::io::copy(&mut BufReader::new(input), &mut encoder)?;
        encoder.finish()?.sync_all()?;
        std::fs::remove_file(&open)?;
        Ok(())
    }

    fn read_lines(reader: impl BufRead, log: &mut ReadLog) {
        for line in reader.split(b'\n') {
            let Ok(line) = line else {
                log.skipped_lines += 1;
                continue;
            };
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match serde_json::from_slice::<Event>(&line) {
                Ok(event) => log.events.push(event),
                Err(_) => log.skipped_lines += 1,
            }
        }
    }

    /// The chat's state, read from its log the first time
    fn state(&self, chat: &str, guard: &mut Option<ChatState>) -> Result<()> {
        if guard.is_none() {
            let log = self.read(chat)?;
            let open_bytes = std::fs::metadata(self.open_segment(chat)).map(|m| m.len()).unwrap_or(0);
            *guard = Some(ChatState { next_seq: log.events.last().map_or(1, |e| e.seq + 1), open_bytes });
        }
        Ok(())
    }
}

impl EventStore for JsonlStore {
    fn append(&self, chat: &str, events: Vec<NewEvent>) -> Result<Vec<Event>> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        let lock = self.lock(chat);
        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(self.chat_dir(chat))?;
        self.state(chat, &mut guard)?;
        let state = guard.as_mut().context("chat state")?;
        if state.open_bytes >= self.segment_bytes {
            self.rotate(chat)?;
            state.open_bytes = 0;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(self.open_segment(chat))?;
        // A cut line from a crash must not swallow the next event
        if state.open_bytes > 0 {
            let mut last = [0u8; 1];
            let mut reader = File::open(self.open_segment(chat))?;
            use std::io::{Seek, SeekFrom};
            reader.seek(SeekFrom::End(-1))?;
            reader.read_exact(&mut last)?;
            if last[0] != b'\n' {
                file.write_all(b"\n")?;
                state.open_bytes += 1;
            }
        }
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut written = Vec::with_capacity(events.len());
        let mut buffer = Vec::new();
        for mut new in events {
            // The kind's own fields cannot overwrite the fields of the event itself
            for reserved in ["v", "id", "chat", "seq", "ts", "kind"] {
                if new.data.remove(reserved).is_some() {
                    tracing::warn!("⚠️ [Memory] Field '{}' of a '{}' event dropped (reserved)", reserved, new.kind);
                }
            }
            self.move_large_fields(&mut new)?;
            let event = Event {
                v: EVENT_VERSION,
                id: format!("ev_{}", uuid::Uuid::new_v4().simple()),
                chat: chat.to_string(),
                seq: state.next_seq,
                ts: new.ts.unwrap_or_else(|| now.clone()),
                kind: new.kind,
                data: new.data,
            };
            state.next_seq += 1;
            serde_json::to_writer(&mut buffer, &event)?;
            buffer.push(b'\n');
            written.push(event);
        }
        let before = (self.closed_segments(chat).len(), state.open_bytes);
        file.write_all(&buffer)?;
        file.flush()?;
        state.open_bytes += buffer.len() as u64;
        // A hot log that matched the files before this append gets the new events
        let mut hot = self.hot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = hot.iter().position(|(id, _, _)| id == chat) {
            if hot[at].1 == before {
                hot[at].1 = (before.0, state.open_bytes);
                Arc::make_mut(&mut hot[at].2).events.extend(written.iter().cloned());
            } else {
                hot.remove(at);
            }
        }
        Ok(written)
    }

    fn sync(&self, chat: &str) -> Result<()> {
        let lock = self.lock(chat);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        match OpenOptions::new().append(true).open(self.open_segment(chat)) {
            Ok(file) => Ok(file.sync_all()?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn read(&self, chat: &str) -> Result<ReadLog> {
        let mut log = ReadLog::default();
        for segment in self.closed_segments(chat) {
            let decoder = zstd::stream::read::Decoder::new(File::open(&segment)?)?;
            Self::read_lines(BufReader::new(decoder), &mut log);
        }
        match File::open(self.open_segment(chat)) {
            Ok(file) => Self::read_lines(BufReader::new(file), &mut log),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if log.skipped_lines > 0 {
            tracing::warn!("⚠️ [Memory] Chat {}: {} unreadable event lines skipped", chat, log.skipped_lines);
        }
        Ok(log)
    }

    fn read_shared(&self, chat: &str) -> Result<Arc<ReadLog>> {
        // Under the chat's lock, so no append runs between the stamp and the read
        let lock = self.lock(chat);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let stamp = self.stamp(chat);
        let mut hot = self.hot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = hot.iter().position(|(id, _, _)| id == chat) {
            let entry = hot.remove(at);
            if entry.1 == stamp {
                let log = Arc::clone(&entry.2);
                hot.push(entry);
                return Ok(log);
            }
        }
        drop(hot);
        let log = Arc::new(self.read(chat)?);
        let mut hot = self.hot.lock().unwrap_or_else(|e| e.into_inner());
        hot.push((chat.to_string(), stamp, Arc::clone(&log)));
        if hot.len() > HOT_LOGS {
            hot.remove(0);
        }
        Ok(log)
    }

    fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        let hex = format!("{:x}", Sha256::digest(bytes));
        let path = self.blob_path(&hex);
        if !path.exists() {
            std::fs::create_dir_all(path.parent().context("blob folder")?)?;
            let temp = path.with_extension("part");
            std::fs::write(&temp, bytes)?;
            std::fs::rename(&temp, &path)?;
        }
        Ok(format!("sha256:{hex}"))
    }

    fn get_blob(&self, reference: &str) -> Result<Vec<u8>> {
        let hex = reference.strip_prefix("sha256:").context("not a blob reference")?;
        anyhow::ensure!(hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()), "not a blob reference");
        Ok(std::fs::read(self.blob_path(hex))?)
    }

    fn delete_chat(&self, chat: &str) -> Result<DeleteReport> {
        let lock = self.lock(chat);
        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut report = DeleteReport::default();
        let mut files: Vec<PathBuf> = self.closed_segments(chat);
        files.push(self.open_segment(chat));
        for file in files {
            if let Ok(meta) = std::fs::metadata(&file) {
                std::fs::remove_file(&file)?;
                report.files += 1;
                report.bytes += meta.len();
            }
        }
        *guard = None;
        self.hot.lock().unwrap_or_else(|e| e.into_inner()).retain(|(id, _, _)| id != chat);
        Ok(report)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    fn temp_store() -> (JsonlStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("cluaiz-memory-test-{}", uuid::Uuid::new_v4().simple()));
        (JsonlStore::new(root.clone()), root)
    }

    fn message(role: &str, text: &str) -> NewEvent {
        NewEvent::new("message", json!({ "role": role, "text": text }))
    }

    /// The contract every backend must meet
    pub(crate) fn conformance(store: &dyn EventStore) {
        let chat = "conformance-chat";
        let first = store.append(chat, vec![message("user", "hi"), message("assistant", "hello")]).unwrap();
        assert_eq!(first.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
        assert_ne!(first[0].id, first[1].id);
        let more = store.append(chat, vec![message("user", "again")]).unwrap();
        assert_eq!(more[0].seq, 3);
        store.sync(chat).unwrap();
        let log = store.read(chat).unwrap();
        assert_eq!(log.events.len(), 3);
        // A shared read sees the same events, and later appends too
        assert_eq!(store.read_shared(chat).unwrap().events.len(), 3);
        store.append(chat, vec![message("assistant", "ok")]).unwrap();
        let shared = store.read_shared(chat).unwrap();
        assert_eq!(shared.events.len(), 4);
        assert_eq!(shared.events[3].seq, 4);
        assert_eq!(store.read(chat).unwrap().events, shared.events);
        assert_eq!(log.events[2].str("text"), Some("again"));
        assert_eq!(log.skipped_lines, 0);
        // Large fields become blobs and come back whole
        let big = "x".repeat(BLOB_BYTES + 1);
        let stored = store.append(chat, vec![NewEvent::new("tool_result", json!({ "result": big }))]).unwrap();
        let reference = stored[0].data["result"]["blob"].as_str().unwrap().to_string();
        assert_eq!(store.get_blob(&reference).unwrap(), big.as_bytes());
        assert!(store.get_blob("sha256:../../etc").is_err());
        // Other chats are apart; deleting removes the log
        assert!(store.read("other-chat").unwrap().events.is_empty());
        assert!(store.delete_chat(chat).unwrap().files >= 1);
        assert!(store.read(chat).unwrap().events.is_empty());
        assert_eq!(store.append(chat, vec![message("user", "new")]).unwrap()[0].seq, 1);
    }

    #[test]
    fn test_jsonl_store_conformance() {
        let (store, root) = temp_store();
        conformance(&store);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_reserved_fields_cannot_overwrite_the_event() {
        let (store, root) = temp_store();
        let written = store.append("c", vec![NewEvent::new("card_enrichment", json!({ "kind": "cold", "seq": 99, "title": "x" }))]).unwrap();
        assert_eq!((written[0].kind.as_str(), written[0].seq), ("card_enrichment", 1));
        let log = store.read("c").unwrap();
        assert_eq!(log.events[0].kind, "card_enrichment");
        assert_eq!(log.events[0].str("title"), Some("x"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_a_line_cut_by_a_crash_is_skipped_and_reported() {
        let (store, root) = temp_store();
        store.append("c", vec![message("user", "one"), message("assistant", "two")]).unwrap();
        // Power cut during the next append: half a line on disk
        let path = store.open_segment("c");
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(br#"{"v":1,"id":"ev_x","chat":"c","seq":3,"ts":"2026-10-24T00:00:00Z","kind":"mess"#).unwrap();
        drop(file);
        let fresh = JsonlStore::new(root.clone());
        let log = fresh.read("c").unwrap();
        assert_eq!((log.events.len(), log.skipped_lines), (2, 1));
        // The next append starts on a new line and continues the sequence
        let next = fresh.append("c", vec![message("user", "three")]).unwrap();
        assert_eq!(next[0].seq, 3);
        let log = fresh.read("c").unwrap();
        assert_eq!((log.events.len(), log.skipped_lines), (3, 1));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_full_segments_are_compressed_and_still_read() {
        let (mut store, root) = temp_store();
        store.segment_bytes = 64 * 1024;
        let text = "y".repeat(4000);
        let appended = 40;
        for _ in 0..appended {
            store.append("c", vec![message("user", &text)]).unwrap();
        }
        assert_eq!(store.closed_segments("c").len(), 2);
        let log = store.read("c").unwrap();
        assert_eq!(log.events.len(), appended);
        assert_eq!(log.events.last().unwrap().seq, appended as u64);
        let _ = std::fs::remove_dir_all(root);
    }
}
