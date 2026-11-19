//! Live notices of a chat-model load while it runs: started (with the plan), each step (weights
//! read in percent, a llama.cpp fallback, context, warmup), a retry in a smaller placement, and how
//! it ended; also unloads. The CLI draws them, the API turns them into task progress and Engine
//! events. Nobody listening costs nothing: a notice without subscribers is dropped. The finished
//! action is kept in `load_history`; these are only the live view.

use serde::Serialize;
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Notices a slow subscriber can fall behind by before it misses some (a load sends about 110)
const BACKLOG: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Started,
    Step,
    Retry,
    Loaded,
    Failed,
    Refused,
    Unloaded,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LoadEvent {
    pub kind: Option<Kind>,
    /// Installed model id
    pub model: String,
    /// llama.cpp load of this action, from 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u8>,
    /// `weights`, `fallback`, `context`, `warmup` (kind `step`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<&'static str>,
    /// Weights read, in percent (step `weights`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_cpu_moe: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_ctx: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_planned_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_taken_gb: Option<f64>,
    /// Time since the action started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    /// A fallback's kind, or why the load failed, was refused or is retried
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl LoadEvent {
    /// Engine event name: `model.load_started`, `model.load_step`, … , `model.unloaded`
    pub fn name(&self) -> &'static str {
        match self.kind {
            Some(Kind::Started) => "model.load_started",
            Some(Kind::Step) => "model.load_step",
            Some(Kind::Retry) => "model.load_retry",
            Some(Kind::Loaded) => "model.loaded",
            Some(Kind::Failed) => "model.load_failed",
            Some(Kind::Refused) => "model.load_refused",
            Some(Kind::Unloaded) | None => "model.unloaded",
        }
    }

    /// The event as JSON, with its terminal line as `message`
    pub fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::to_value(self).unwrap_or_default();
        json["message"] = serde_json::Value::String(self.describe());
        json
    }

    /// One line for a terminal: `reading weights 37%`, `retry 2: MoeSplit, n_cpu_moe 48`
    pub fn describe(&self) -> String {
        match (self.kind, self.step) {
            (Some(Kind::Started), _) => format!(
                "loading: {} on the plan, {} tokens",
                self.placement.as_deref().unwrap_or("?"),
                self.n_ctx.unwrap_or(0)
            ),
            (Some(Kind::Step), Some("weights")) => format!("reading weights {}%", self.percent.unwrap_or(0)),
            (Some(Kind::Step), Some("fallback")) => format!("llama.cpp retrying {}", self.detail.as_deref().unwrap_or("")),
            (Some(Kind::Step), Some("context")) => "making KV cache and compute buffers".to_string(),
            (Some(Kind::Step), Some("warmup")) => "warming up".to_string(),
            (Some(Kind::Retry), _) => format!(
                "did not fit; retry {}: {}, n_cpu_moe {}",
                self.attempt.unwrap_or(2),
                self.placement.as_deref().unwrap_or("?"),
                self.n_cpu_moe.unwrap_or(0)
            ),
            (Some(Kind::Loaded), _) => format!("loaded in {} ms", self.ms.unwrap_or(0)),
            (Some(Kind::Failed), _) | (Some(Kind::Refused), _) => {
                format!("not loaded: {}", self.detail.as_deref().unwrap_or(""))
            }
            (Some(Kind::Unloaded), _) => format!("unloaded in {} ms", self.ms.unwrap_or(0)),
            _ => String::new(),
        }
    }
}

fn bus() -> &'static broadcast::Sender<LoadEvent> {
    static BUS: OnceLock<broadcast::Sender<LoadEvent>> = OnceLock::new();
    BUS.get_or_init(|| broadcast::channel(BACKLOG).0)
}

pub fn publish(event: LoadEvent) {
    let sender = bus();
    if sender.receiver_count() > 0 {
        let _ = sender.send(event);
    }
}

pub fn subscribe() -> broadcast::Receiver<LoadEvent> {
    bus().subscribe()
}

/// The event of a llama.cpp load step
pub fn of_step(model: &str, attempt: u8, step: llama_engine::native::load_steps::LoadStep) -> LoadEvent {
    use llama_engine::native::load_steps::LoadStep;
    let (name, percent, detail) = match step {
        LoadStep::Weights(percent) => ("weights", Some(percent), None),
        LoadStep::Fallback(what) => ("fallback", None, Some(what.to_string())),
        LoadStep::Context => ("context", None, None),
        LoadStep::Warmup => ("warmup", None, None),
    };
    LoadEvent { kind: Some(Kind::Step), model: model.to_string(), attempt: Some(attempt), step: Some(name), percent, detail, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_get_steps_with_their_line() {
        let mut events = subscribe();
        publish(of_step("m", 1, llama_engine::native::load_steps::LoadStep::Weights(37)));
        let event = events.recv().await.unwrap();
        assert_eq!((event.name(), event.describe().as_str()), ("model.load_step", "reading weights 37%"));
        let json = event.to_json();
        assert_eq!((json["kind"].as_str(), json["percent"].as_u64(), json.get("detail")), (Some("step"), Some(37), None));
        assert_eq!(json["message"], "reading weights 37%");
    }
}
