//! Idle-time work (architecture §13): small jobs that run when no chat request is in flight and
//! stop at once when one arrives.
//!
//! The one job so far is card enrichment: after an answer, while the chat's KV is still in the
//! context, the model writes a title, motive and summary for the turn (at most 80 tokens,
//! thinking off). It is an ordinary request that shares the conversation's prefix, the way
//! llama.cpp's prompt cache works; the next real request keeps the same prefix and drops the
//! rest, so nothing is rolled back by hand. It never loads a model, runs only when the model that
//! answered is still loaded, and only with GPU layers unless the policy says otherwise.

use crate::state::AppState;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Most tokens an enrichment may write
const ENRICH_TOKENS: usize = 80;
/// Quiet time before a job starts after the last request ended
const IDLE_MS: u64 = 300;

/// A card enrichment for one turn
pub struct EnrichJob {
    pub chat: String,
    /// The turn's prompt envelope (JSON) with the answer appended as an assistant message
    pub envelope: Value,
    pub user_pos: usize,
    pub answer_pos: usize,
    pub model_path: std::path::PathBuf,
    pub settings: cluaiz_core::ActiveUserSettings,
}

struct Scheduler {
    queue: Mutex<VecDeque<EnrichJob>>,
    /// Chat requests in flight
    foreground: AtomicUsize,
    /// Stop flag of the running job
    running: Mutex<Option<Arc<AtomicBool>>>,
    wake: tokio::sync::Notify,
    started: std::sync::Once,
}

static SCHEDULER: LazyLock<Scheduler> = LazyLock::new(|| Scheduler {
    queue: Mutex::new(VecDeque::new()),
    foreground: AtomicUsize::new(0),
    running: Mutex::new(None),
    wake: tokio::sync::Notify::new(),
    started: std::sync::Once::new(),
});

/// Held by a chat request from its start to its end; a running job stops when one begins
pub struct Foreground(());

impl Foreground {
    pub fn begin() -> Self {
        SCHEDULER.foreground.fetch_add(1, Ordering::SeqCst);
        if let Some(stop) = SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            stop.store(true, Ordering::SeqCst);
        }
        Foreground(())
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        if SCHEDULER.foreground.fetch_sub(1, Ordering::SeqCst) == 1 {
            SCHEDULER.wake.notify_one();
        }
    }
}

/// Queues a job (only the newest turn of a chat is kept waiting) and starts the worker once
pub fn enqueue(state: &Arc<AppState>, job: EnrichJob) {
    {
        let mut queue = SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.retain(|queued| queued.chat != job.chat);
        queue.push_back(job);
    }
    let state = Arc::clone(state);
    SCHEDULER.started.call_once(move || {
        tokio::spawn(worker(state));
    });
    SCHEDULER.wake.notify_one();
}

async fn worker(state: Arc<AppState>) {
    loop {
        SCHEDULER.wake.notified().await;
        loop {
            // Foreground first: wait until no request is in flight, then a little longer
            while SCHEDULER.foreground.load(Ordering::SeqCst) > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(IDLE_MS)).await;
            if SCHEDULER.foreground.load(Ordering::SeqCst) > 0 {
                continue;
            }
            let Some(job) = SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front() else { break };
            let stop = Arc::new(AtomicBool::new(false));
            *SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&stop));
            run_enrichment(&state, job, stop).await;
            *SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

/// Whether enrichment may run now: policy, a loaded model with GPU layers (or the policy allows
/// CPU), and the same model that answered
async fn allowed(state: &Arc<AppState>, job: &EnrichJob) -> bool {
    match cluaiz_core::Settings::current().context_runtime.indexer.as_str() {
        "off" | "rules_only" => return false,
        "always" => {}
        _ => {
            let gpu = state.host.current_plan.lock().await.as_ref().is_some_and(|plan| plan.n_gpu_layers != 0);
            if !gpu {
                return false;
            }
        }
    }
    state.host.active_model_path.lock().await.as_deref() == Some(&*job.model_path.to_string_lossy())
}

const INSTRUCTION: &str = "Index the last exchange for search. Reply with only one JSON object and nothing else: \
{\"title\": \"at most 10 words\", \"motive\": \"why the user asked, at most 12 words\", \"summary\": \"at most 2 short sentences\", \
\"keywords\": [\"up to 6 words, in English and in the user's language\"]}";

async fn run_enrichment(state: &Arc<AppState>, job: EnrichJob, stop: Arc<AtomicBool>) {
    if !allowed(state, &job).await {
        return;
    }
    let started = std::time::Instant::now();
    let mut envelope = job.envelope;
    if let Some(messages) = envelope.get_mut("messages").and_then(Value::as_array_mut) {
        messages.push(json!({ "role": "user", "content": INSTRUCTION }));
    }
    envelope["think_mode"] = json!("off");
    envelope["kv_reuse"] = json!("prefix");
    let response = state
        .host
        .dispatch_chat_text(
            &envelope.to_string(),
            ENRICH_TOKENS,
            &job.model_path,
            &job.settings,
            Arc::clone(&stop),
            Arc::new(AtomicBool::new(false)),
            Some(&job.chat),
            false,
        )
        .await;
    let mut text = String::new();
    if let cluaiz_runtime::HostResponse::TokenStream(mut rx) = response {
        while let Some(token) = rx.recv().await {
            if cluaiz_runtime::stream_error(&token).is_some() {
                return;
            }
            text.push_str(&token);
        }
    }
    if stop.load(Ordering::SeqCst) {
        tracing::info!("🗂️ [Indexer] Enrichment of chat {} stopped for a request", job.chat);
        return;
    }
    let Some(card) = parse_card(&text) else {
        tracing::info!("🗂️ [Indexer] Chat {}: no card in {:?}", job.chat, text.chars().take(120).collect::<String>());
        return;
    };
    let context = state.host.last_context().await;
    tracing::info!(
        "🗂️ [Indexer] Chat {} turn {}: {:?} in {} ms (cache {} of {} prompt tokens)",
        job.chat,
        job.answer_pos,
        card.get("title"),
        started.elapsed().as_millis(),
        context.as_ref().map_or(0, |c| c.cache_n),
        context.as_ref().map_or(0, |c| c.prompt_tokens)
    );
    let model = job.model_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut data = card;
    data["user_pos"] = json!(job.user_pos);
    data["answer_pos"] = json!(job.answer_pos);
    data["by"] = json!(format!("model:{model}"));
    data["prompt"] = json!("card-v1");
    use cluaiz_agents::memory::EventStore;
    let store = cluaiz_agents::memory::JsonlStore::shared();
    if let Err(e) = store.append(&job.chat, vec![cluaiz_agents::memory::NewEvent::new("card_enrichment", data)]) {
        tracing::warn!("⚠️ [Indexer] Could not store the card of chat {}: {}", job.chat, e);
    }
}

/// The card fields from the model's reply (the first JSON object in it), trimmed to size
fn parse_card(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let value: Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    let field = |key: &str, words: usize| -> Option<String> {
        let text = value.get(key)?.as_str()?.trim();
        (!text.is_empty()).then(|| text.split_whitespace().take(words).collect::<Vec<_>>().join(" "))
    };
    let title = field("title", 12)?;
    let keywords: Vec<String> = value
        .get("keywords")
        .and_then(Value::as_array)
        .map(|k| k.iter().filter_map(Value::as_str).take(8).map(str::to_string).collect())
        .unwrap_or_default();
    Some(json!({ "title": title, "motive": field("motive", 16), "summary": field("summary", 60), "keywords": keywords }))
}
