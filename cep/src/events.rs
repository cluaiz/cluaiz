//! Engine events: short notices that something happened (a task ended, an approval was asked or
//! answered, a tool was installed). Layers outside the Engine subscribe to them; Automation starts
//! flows on them. Events carry ids and names, never content (no chat text, no file data).
//! Nobody listening costs nothing: a notice without subscribers is dropped.

use serde_json::Value;
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Notices a slow subscriber can fall behind by before it misses some
const BACKLOG: usize = 1024;

#[derive(Debug, Clone)]
pub struct Event {
    /// `task.done`, `approval.decided`, …
    pub name: String,
    pub data: Value,
    /// RFC 3339, when it happened
    pub at: String,
}

fn bus() -> &'static broadcast::Sender<Event> {
    static BUS: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    BUS.get_or_init(|| broadcast::channel(BACKLOG).0)
}

pub fn publish(name: &str, data: Value) {
    let sender = bus();
    if sender.receiver_count() > 0 {
        let at = crate::approval::rfc3339(crate::approval::now_ms());
        let _ = sender.send(Event { name: name.to_string(), data, at });
    }
}

pub fn subscribe() -> broadcast::Receiver<Event> {
    bus().subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn subscribers_get_what_is_published_after_they_subscribe() {
        publish("nobody.listens", json!({}));
        let mut events = subscribe();
        publish("task.done", json!({ "task_id": "t_1" }));
        let event = events.recv().await.unwrap();
        assert_eq!((event.name.as_str(), event.data["task_id"].as_str()), ("task.done", Some("t_1")));
    }
}
