//! Engine events: short notices that something happened (a task ended, an approval was asked or
//! answered, a tool was installed, a model load moved a step). Layers outside the Engine subscribe
//! to them; Automation starts flows on them. Events carry ids and names, never content (no chat
//! text, no file data).
//!
//! Two ways to read them:
//! - `subscribe()`: a live receiver inside the process. With nobody subscribed a live notice is not
//!   sent.
//! - `read_since` / `wait_since` (CEP `system.events`): every notice also goes into a bounded
//!   in-memory ring with a sequence number, so a client outside the process (the App, a script)
//!   can read by cursor and long-poll. The ring is lost when the Engine stops.

use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};

/// Notices a slow subscriber can fall behind by before it misses some
const BACKLOG: usize = 1024;
/// Notices the ring keeps for `system.events`. A chat-model load sends about 110
/// (`engine/runtime/src/load_events.rs`), so this holds the last 9 or so loads; at a few hundred
/// bytes a notice it stays well under 1 MB.
pub const RING_CAPACITY: usize = 1024;
/// Longest `wait_since` wait. A long poll longer than this mostly meets proxy and client
/// timeouts; the App waits 15 s and gives the call `wait_ms + 5 s`.
pub const MAX_WAIT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct Event {
    /// `task.done`, `approval.decided`, …
    pub name: String,
    pub data: Value,
    /// RFC 3339, when it happened
    pub at: String,
}

/// One notice in the ring, as `system.events` returns it
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordedEvent {
    /// From 1, one higher for each notice of this Engine run
    pub seq: u64,
    pub name: String,
    /// RFC 3339
    pub at: String,
    pub data: Value,
}

/// A page of `system.events`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventsPage {
    /// Oldest first
    pub events: Vec<RecordedEvent>,
    /// Pass as `after` next time: the last returned `seq`, or `after` when nothing was returned
    pub next: u64,
    /// The newest `seq` of this Engine run (0 before the first notice)
    pub latest_seq: u64,
}

/// The newest notices with their sequence numbers, and a watch that tells waiters the newest seq
pub struct Ring {
    capacity: usize,
    state: Mutex<RingState>,
    latest: watch::Sender<u64>,
}

struct RingState {
    latest_seq: u64,
    records: VecDeque<RecordedEvent>,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            state: Mutex::new(RingState { latest_seq: 0, records: VecDeque::with_capacity(capacity) }),
            latest: watch::channel(0).0,
        }
    }

    /// Keeps the notice (dropping the oldest when full) and returns its seq
    pub fn push(&self, name: &str, data: Value, at: String) -> u64 {
        let seq = {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.latest_seq += 1;
            let seq = state.latest_seq;
            if state.records.len() >= self.capacity {
                state.records.pop_front();
            }
            state.records.push_back(RecordedEvent { seq, name: name.to_string(), at, data });
            seq
        };
        self.latest.send_replace(seq);
        seq
    }

    /// Notices with `seq > after`, oldest first, at most `limit`
    pub fn read_since(&self, after: u64, limit: usize) -> EventsPage {
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // seq is strictly increasing in the ring, so skip by position instead of scanning
        let start = state.records.partition_point(|event| event.seq <= after);
        let events: Vec<RecordedEvent> = state.records.iter().skip(start).take(limit).cloned().collect();
        let next = events.last().map_or(after, |event| event.seq);
        EventsPage { events, next, latest_seq: state.latest_seq }
    }

    /// Like `read_since`, but when nothing is newer than `after`, waits up to `wait_ms` (capped at
    /// `MAX_WAIT_MS`) for a notice and returns as soon as one arrives
    pub async fn wait_since(&self, after: u64, limit: usize, wait_ms: u64) -> EventsPage {
        // Subscribe before reading, so a notice pushed between the read and the wait is not missed
        let mut latest = self.latest.subscribe();
        let page = self.read_since(after, limit);
        if !page.events.is_empty() || wait_ms == 0 {
            return page;
        }
        let wait = Duration::from_millis(wait_ms.min(MAX_WAIT_MS));
        let _ = tokio::time::timeout(wait, latest.wait_for(|seq| *seq > after)).await;
        self.read_since(after, limit)
    }
}

fn bus() -> &'static broadcast::Sender<Event> {
    static BUS: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    BUS.get_or_init(|| broadcast::channel(BACKLOG).0)
}

fn ring() -> &'static Ring {
    static RING: OnceLock<Ring> = OnceLock::new();
    RING.get_or_init(|| Ring::new(RING_CAPACITY))
}

pub fn publish(name: &str, data: Value) {
    let at = crate::approval::rfc3339(crate::approval::now_ms());
    let sender = bus();
    if sender.receiver_count() > 0 {
        let _ = sender.send(Event { name: name.to_string(), data: data.clone(), at: at.clone() });
    }
    ring().push(name, data, at);
}

pub fn subscribe() -> broadcast::Receiver<Event> {
    bus().subscribe()
}

/// The Engine's ring: notices with `seq > after`, oldest first, at most `limit`
pub fn read_since(after: u64, limit: usize) -> EventsPage {
    ring().read_since(after, limit)
}

/// The Engine's ring, waiting up to `wait_ms` (capped at `MAX_WAIT_MS`) when nothing is newer
pub async fn wait_since(after: u64, limit: usize, wait_ms: u64) -> EventsPage {
    ring().wait_since(after, limit, wait_ms).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;

    fn names(page: &EventsPage) -> Vec<&str> {
        page.events.iter().map(|event| event.name.as_str()).collect()
    }

    #[tokio::test]
    async fn subscribers_get_what_is_published_after_they_subscribe() {
        // The bus is process-wide and other tests publish too, so look for this test's own name
        let mut events = subscribe();
        publish("test.subscriber_sees_this", json!({ "task_id": "t_1" }));
        loop {
            let event = events.recv().await.unwrap();
            if event.name == "test.subscriber_sees_this" {
                assert_eq!(event.data["task_id"], "t_1");
                break;
            }
        }
    }

    #[test]
    fn publish_keeps_the_notice_in_the_engine_ring_even_with_nobody_subscribed() {
        let before = read_since(u64::MAX, 1).latest_seq;
        publish("test.kept_without_subscribers", json!({ "model": "m" }));
        let page = read_since(before, RING_CAPACITY);
        assert!(page.events.iter().any(|event| event.name == "test.kept_without_subscribers" && event.data["model"] == "m"));
        assert!(page.latest_seq > before);
    }

    #[test]
    fn cursor_pages_in_order_and_limit_caps_the_page() {
        let ring = Ring::new(16);
        for n in 1..=5 {
            ring.push(&format!("e{n}"), json!({}), "2026-11-22T00:00:00Z".into());
        }
        let first = ring.read_since(0, 2);
        assert_eq!((names(&first), first.next, first.latest_seq), (vec!["e1", "e2"], 2, 5));
        let second = ring.read_since(first.next, 2);
        assert_eq!((names(&second), second.next), (vec!["e3", "e4"], 4));
        let third = ring.read_since(second.next, 2);
        assert_eq!((names(&third), third.next), (vec!["e5"], 5));
        let empty = ring.read_since(third.next, 2);
        assert_eq!((empty.events.len(), empty.next, empty.latest_seq), (0, 5, 5));
        // A cursor from an earlier Engine run is above latest_seq: nothing, next stays the cursor
        let stale = ring.read_since(99, 2);
        assert_eq!((stale.events.len(), stale.next, stale.latest_seq), (0, 99, 5));
    }

    #[test]
    fn a_full_ring_drops_the_oldest_and_seq_keeps_counting() {
        let ring = Ring::new(3);
        for n in 1..=5 {
            ring.push(&format!("e{n}"), json!({}), String::new());
        }
        let page = ring.read_since(0, 10);
        assert_eq!(names(&page), vec!["e3", "e4", "e5"]);
        assert_eq!(page.events.iter().map(|event| event.seq).collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!((page.next, page.latest_seq), (5, 5));
    }

    #[tokio::test]
    async fn wait_returns_early_when_a_notice_arrives() {
        let ring = Arc::new(Ring::new(8));
        let pusher = ring.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            pusher.push("model.loaded", json!({ "model": "m" }), String::new());
        });
        let started = Instant::now();
        let page = ring.wait_since(0, 10, 5_000).await;
        assert_eq!((names(&page), page.next), (vec!["model.loaded"], 1));
        assert!(started.elapsed() < Duration::from_millis(2_000), "waited {:?}", started.elapsed());
    }

    #[tokio::test]
    async fn wait_times_out_empty_and_zero_wait_does_not_wait() {
        let ring = Ring::new(8);
        ring.push("e1", json!({}), String::new());
        let started = Instant::now();
        let page = ring.wait_since(1, 10, 100).await;
        let waited = started.elapsed();
        assert_eq!((page.events.len(), page.next, page.latest_seq), (0, 1, 1));
        assert!(waited >= Duration::from_millis(90) && waited < Duration::from_millis(2_000), "waited {waited:?}");
        let started = Instant::now();
        let page = ring.wait_since(1, 10, 0).await;
        assert!(page.events.is_empty() && started.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn wait_returns_at_once_when_something_is_already_newer() {
        let ring = Ring::new(8);
        ring.push("e1", json!({}), String::new());
        let started = Instant::now();
        let page = ring.wait_since(0, 10, 5_000).await;
        assert_eq!(names(&page), vec!["e1"]);
        assert!(started.elapsed() < Duration::from_millis(50));
    }
}
