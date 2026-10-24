//! Idle-time work (architecture §13): small jobs that run when no chat request is in flight and
//! stop at once when one arrives.
//!
//! The one job so far is card enrichment: the model writes a title, motive and summary for a
//! turn (at most 80 tokens, thinking off). Right after an answer (a "hot" job) it runs on the
//! chat's KV, as an ordinary request that shares the conversation's prefix, the way llama.cpp's
//! prompt cache works; the next real request keeps the same prefix and drops the rest. It never
//! loads a model, and by default runs only with GPU layers.
//!
//! Nothing is lost on a restart or a crash: the chat's event log is the job list. A card is
//! written only when it is done, and turns without one are found again by a scan ("cold" jobs:
//! only that turn is sent). Cold jobs run after a longer quiet time, a few at a time, and only
//! when chat memory is on, because they replace the KV of the chat the user was in (which then
//! comes back from disk).

use crate::state::AppState;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Most tokens an enrichment may write
const ENRICH_TOKENS: usize = 80;
/// Quiet time before a job starts after the last request ended
const IDLE_MS: u64 = 300;
/// Quiet time before turns without a card are looked for
const COLD_IDLE: std::time::Duration = std::time::Duration::from_secs(60);
/// Turns without a card queued by one scan
const COLD_BATCH: usize = 20;
/// Only chats written to within this time are scanned
const COLD_RECENT_DAYS: u64 = 7;
/// Results kept for `GET /v1/context/jobs`
const RECENT_RESULTS: usize = 20;

/// A card enrichment for one turn
pub struct EnrichJob {
    pub chat: String,
    /// The prompt envelope (JSON) with the answer appended as an assistant message: the whole
    /// turn for a hot job, only the question and the answer for a cold one
    pub envelope: Value,
    pub user_pos: usize,
    pub answer_pos: usize,
    /// None for a cold job: the loaded model is used
    pub model_path: Option<std::path::PathBuf>,
    pub settings: Option<cluaiz_core::ActiveUserSettings>,
    pub cold: bool,
    /// Set for an episode summary (id, first and last position): the envelope already holds the
    /// whole request
    pub episode: Option<(String, u64, u64)>,
}

/// What happened to a job, for `GET /v1/context/jobs`
#[derive(Clone, serde::Serialize)]
struct JobResult {
    chat: String,
    answer_pos: usize,
    cold: bool,
    outcome: String,
    ms: u128,
}

#[derive(Default, serde::Serialize)]
struct Counts {
    done: u64,
    failed: u64,
    stopped: u64,
    skipped: u64,
}

struct Scheduler {
    queue: Mutex<VecDeque<EnrichJob>>,
    /// When the last chat request ended
    last_foreground: Mutex<std::time::Instant>,
    counts: Mutex<Counts>,
    recent: Mutex<VecDeque<JobResult>>,
    /// Chat requests in flight
    foreground: AtomicUsize,
    /// Stop flag of the running job
    running: Mutex<Option<Arc<AtomicBool>>>,
    wake: tokio::sync::Notify,
    started: std::sync::Once,
}

static SCHEDULER: LazyLock<Scheduler> = LazyLock::new(|| Scheduler {
    queue: Mutex::new(VecDeque::new()),
    last_foreground: Mutex::new(std::time::Instant::now()),
    counts: Mutex::new(Counts::default()),
    recent: Mutex::new(VecDeque::new()),
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
        *SCHEDULER.last_foreground.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now();
        if SCHEDULER.foreground.fetch_sub(1, Ordering::SeqCst) == 1 {
            SCHEDULER.wake.notify_one();
        }
    }
}

/// Starts the worker once (on the first chat request, so a restart resumes on its own)
pub fn ensure_started(state: &Arc<AppState>) {
    let state = Arc::clone(state);
    SCHEDULER.started.call_once(move || {
        tokio::spawn(worker(state));
    });
}

/// Queues a hot job. A hot job of the same chat that is still waiting becomes a cold one later
/// (its KV is no longer hot): the scan finds that turn again, so no turn is dropped.
pub fn enqueue(state: &Arc<AppState>, job: EnrichJob) {
    {
        let mut queue = SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.retain(|queued| queued.cold || queued.chat != job.chat);
        queue.push_back(job);
    }
    ensure_started(state);
    SCHEDULER.wake.notify_one();
}

/// The scheduler's queue, counts and recent results (`GET /v1/context/jobs`)
pub fn status() -> Value {
    let queue: Vec<Value> = SCHEDULER
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|job| json!({ "chat_id": job.chat, "answer_pos": job.answer_pos, "kind": if job.cold { "cold" } else { "hot" } }))
        .collect();
    json!({
        "indexer": cluaiz_core::Settings::current().context_runtime.indexer,
        "requests_in_flight": SCHEDULER.foreground.load(Ordering::SeqCst),
        "running": SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()).is_some(),
        "queue": queue,
        "counts": &*SCHEDULER.counts.lock().unwrap_or_else(|e| e.into_inner()),
        "recent": SCHEDULER.recent.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect::<Vec<_>>(),
    })
}

fn record(job: &EnrichJob, outcome: &str, started: std::time::Instant) {
    {
        let mut counts = SCHEDULER.counts.lock().unwrap_or_else(|e| e.into_inner());
        match outcome {
            "done" => counts.done += 1,
            "stopped" => counts.stopped += 1,
            o if o.starts_with("skipped") => counts.skipped += 1,
            _ => counts.failed += 1,
        }
    }
    let mut recent = SCHEDULER.recent.lock().unwrap_or_else(|e| e.into_inner());
    recent.push_back(JobResult {
        chat: job.chat.clone(),
        answer_pos: job.answer_pos,
        cold: job.cold,
        outcome: outcome.to_string(),
        ms: started.elapsed().as_millis(),
    });
    while recent.len() > RECENT_RESULTS {
        recent.pop_front();
    }
}

/// Chats written to recently, newest first, with their ids (read from the first event)
fn recent_chats() -> Vec<String> {
    chats_within(COLD_RECENT_DAYS)
}

/// Chats written to within `days`, newest first, with their ids (read from the first event)
pub fn chats_within(days: u64) -> Vec<String> {
    let root = cluaiz_agents::memory::memory_root().join("chats");
    let mut chats: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let log = entry.path().join("events.jsonl");
            let modified = std::fs::metadata(&log).and_then(|m| m.modified()).ok()?;
            if modified.elapsed().ok()?.as_secs() > days * 86_400 {
                return None;
            }
            use std::io::BufRead;
            let first = std::io::BufReader::new(std::fs::File::open(&log).ok()?).lines().next()?.ok()?;
            let chat = serde_json::from_str::<Value>(&first).ok()?.get("chat")?.as_str()?.to_string();
            Some((modified, chat))
        })
        .collect();
    chats.sort_by(|a, b| b.0.cmp(&a.0));
    chats.into_iter().map(|(_, chat)| chat).collect()
}

/// Turns without a card, as cold jobs (at most `COLD_BATCH`, newest chats first)
fn scan_unindexed() -> Vec<EnrichJob> {
    use cluaiz_agents::memory::EventStore;
    let store = cluaiz_agents::memory::JsonlStore::shared();
    let queued: std::collections::HashSet<(String, usize)> = SCHEDULER
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|job| (job.chat.clone(), job.answer_pos))
        .collect();
    let mut jobs = Vec::new();
    for chat in recent_chats() {
        let Ok(log) = store.read(&chat) else { continue };
        for turn in cluaiz_agents::memory::transcript::unindexed_turns(&log.events) {
            if jobs.len() >= COLD_BATCH {
                return jobs;
            }
            if queued.contains(&(chat.clone(), turn.answer_pos as usize)) {
                continue;
            }
            jobs.push(EnrichJob {
                chat: chat.clone(),
                envelope: json!({ "messages": [
                    { "role": "user", "content": turn.user },
                    { "role": "assistant", "content": turn.answer },
                ] }),
                user_pos: turn.user_pos as usize,
                answer_pos: turn.answer_pos as usize,
                model_path: None,
                settings: None,
                cold: true,
                episode: None,
            });
        }
        // Closed episodes (not the current one) of some length get a summary
        let Some(map) = super::recall::chat_map(&chat) else { continue };
        let summarized: std::collections::HashSet<String> = log
            .events
            .iter()
            .filter(|e| e.kind == "episode_summary" || e.kind == "episode_summary_failed")
            .filter_map(|e| e.str("episode").map(str::to_string))
            .collect();
        for episode in map.episodes.iter().rev().skip(1).filter(|e| e.messages >= EPISODE_MIN_MESSAGES) {
            if jobs.len() >= COLD_BATCH {
                return jobs;
            }
            if summarized.contains(&episode.id) {
                continue;
            }
            jobs.push(EnrichJob {
                chat: chat.clone(),
                envelope: json!({ "messages": [{ "role": "user", "content": super::recall::episode_request(&map, episode) }] }),
                user_pos: episode.first_pos as usize,
                answer_pos: episode.last_pos as usize,
                model_path: None,
                settings: None,
                cold: true,
                episode: Some((episode.id.clone(), episode.first_pos, episode.last_pos)),
            });
        }
    }
    jobs
}

/// Episodes shorter than this get no summary
const EPISODE_MIN_MESSAGES: usize = 4;

async fn worker(state: Arc<AppState>) {
    loop {
        // New work wakes the worker; otherwise it looks for turns without a card now and then
        let woke = tokio::time::timeout(COLD_IDLE, SCHEDULER.wake.notified()).await.is_ok();
        if !woke && SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
            let quiet = SCHEDULER.last_foreground.lock().unwrap_or_else(|e| e.into_inner()).elapsed() >= COLD_IDLE;
            if quiet && SCHEDULER.foreground.load(Ordering::SeqCst) == 0 && cold_allowed() {
                let jobs = tokio::task::spawn_blocking(scan_unindexed).await.unwrap_or_default();
                if !jobs.is_empty() {
                    tracing::info!("🗂️ [Indexer] {} earlier turns without a card queued", jobs.len());
                    SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner()).extend(jobs);
                }
            }
        }
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
            // Cold jobs wait for the longer quiet time; a request in between puts them back
            if job.cold && SCHEDULER.last_foreground.lock().unwrap_or_else(|e| e.into_inner()).elapsed() < COLD_IDLE {
                SCHEDULER.queue.lock().unwrap_or_else(|e| e.into_inner()).push_front(job);
                break;
            }
            let stop = Arc::new(AtomicBool::new(false));
            *SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&stop));
            run_enrichment(&state, job, stop).await;
            *SCHEDULER.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

/// Cold jobs replace the KV of the chat the user was in: only when chat memory keeps it on disk
fn cold_allowed() -> bool {
    let settings = cluaiz_core::Settings::current();
    settings.chat_memory.enabled && !matches!(settings.context_runtime.indexer.as_str(), "off" | "rules_only")
}

/// The model and settings a job runs with, when it may run now: policy, a loaded model with GPU
/// layers (or the policy allows CPU), and for a hot job the same model that answered
async fn runnable(state: &Arc<AppState>, job: &EnrichJob) -> Result<(std::path::PathBuf, cluaiz_core::ActiveUserSettings), &'static str> {
    match cluaiz_core::Settings::current().context_runtime.indexer.as_str() {
        "off" | "rules_only" => return Err("skipped: indexer policy"),
        "always" => {}
        _ => {
            let gpu = state.host.current_plan.lock().await.as_ref().is_some_and(|plan| plan.n_gpu_layers != 0);
            if !gpu {
                return Err("skipped: no GPU layers");
            }
        }
    }
    if job.cold && !cold_allowed() {
        return Err("skipped: chat memory is off");
    }
    let loaded = state.host.active_model_path.lock().await.clone().ok_or("skipped: no model loaded")?;
    match (&job.model_path, &job.settings) {
        (Some(path), Some(settings)) if *path.to_string_lossy() == loaded => Ok((path.clone(), settings.clone())),
        (Some(_), _) => Err("skipped: another model is loaded"),
        _ => {
            let settings = state.host.loaded_settings.lock().await.clone().ok_or("skipped: no model loaded")?;
            Ok((std::path::PathBuf::from(loaded), settings))
        }
    }
}

const INSTRUCTION: &str = "Write a search card for my previous message and your answer to it (not for this request). \
Reply with only one JSON object and nothing else: \
{\"title\": \"what that exchange was about, at most 10 words\", \"motive\": \"why I asked, at most 12 words\", \
\"summary\": \"at most 2 short sentences\", \"keywords\": [\"up to 6 words, in English and in my language\"]}";

async fn run_enrichment(state: &Arc<AppState>, job: EnrichJob, stop: Arc<AtomicBool>) {
    let started = std::time::Instant::now();
    let (model_path, settings) = match runnable(state, &job).await {
        Ok(found) => found,
        Err(reason) => {
            record(&job, reason, started);
            return;
        }
    };
    let mut envelope = job.envelope.clone();
    if job.episode.is_none() {
        if let Some(messages) = envelope.get_mut("messages").and_then(Value::as_array_mut) {
            messages.push(json!({ "role": "user", "content": INSTRUCTION }));
        }
    }
    envelope["think_mode"] = json!("off");
    envelope["kv_reuse"] = json!("prefix");
    let response = state
        .host
        .dispatch_chat_text(
            &envelope.to_string(),
            ENRICH_TOKENS,
            &model_path,
            &settings,
            Arc::clone(&stop),
            Arc::new(AtomicBool::new(false)),
            // A cold job does not take over the KV as that chat (no restore of its saved KV)
            if job.cold { None } else { Some(&job.chat) },
            false,
        )
        .await;
    let mut text = String::new();
    if let cluaiz_runtime::HostResponse::TokenStream(mut rx) = response {
        while let Some(token) = rx.recv().await {
            if cluaiz_runtime::stream_error(&token).is_some() {
                // Stopped before it ran, or the engine refused: the turn stays without a card
                let outcome = if stop.load(Ordering::SeqCst) { "stopped" } else { "failed: engine error" };
                record(&job, outcome, started);
                return;
            }
            text.push_str(&token);
        }
    }
    if stop.load(Ordering::SeqCst) {
        // Nothing was written: the turn is picked up again later (hot jobs by the scan)
        tracing::info!("🗂️ [Indexer] Enrichment of chat {} stopped for a request", job.chat);
        record(&job, "stopped", started);
        return;
    }
    use cluaiz_agents::memory::EventStore;
    let store = cluaiz_agents::memory::JsonlStore::shared();
    if let Some((episode, first, last)) = &job.episode {
        let model = model_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let (kind, data, outcome) = match parse_card(&text).filter(|c| c.get("summary").is_some_and(|s| !s.is_null())) {
            Some(mut card) => {
                card["episode"] = json!(episode);
                card["first_pos"] = json!(first);
                card["last_pos"] = json!(last);
                card["by"] = json!(format!("model:{model}"));
                tracing::info!("🗂️ [Indexer] Chat {} {}: {:?}", job.chat, episode, card.get("title"));
                ("episode_summary", card, "done")
            }
            None => ("episode_summary_failed", json!({ "episode": episode, "reply": text.chars().take(200).collect::<String>() }), "failed: no summary in the reply"),
        };
        let _ = store.append(&job.chat, vec![cluaiz_agents::memory::NewEvent::new(kind, data)]);
        record(&job, outcome, started);
        return;
    }
    let Some(card) = parse_card(&text) else {
        tracing::info!("🗂️ [Indexer] Chat {}: no card in {:?}", job.chat, text.chars().take(120).collect::<String>());
        // Recorded, so the scan does not try this turn again and again
        let failed = json!({ "user_pos": job.user_pos, "answer_pos": job.answer_pos, "reply": text.chars().take(200).collect::<String>() });
        let _ = store.append(&job.chat, vec![cluaiz_agents::memory::NewEvent::new("card_enrichment_failed", failed)]);
        record(&job, "failed: no card in the reply", started);
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
    let model = model_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut data = card;
    data["user_pos"] = json!(job.user_pos);
    data["answer_pos"] = json!(job.answer_pos);
    data["by"] = json!(format!("model:{model}"));
    data["prompt"] = json!("card-v1");
    data["job"] = json!(if job.cold { "cold" } else { "hot" });
    match store.append(&job.chat, vec![cluaiz_agents::memory::NewEvent::new("card_enrichment", data)]) {
        Ok(_) => record(&job, "done", started),
        Err(e) => {
            tracing::warn!("⚠️ [Indexer] Could not store the card of chat {}: {}", job.chat, e);
            record(&job, "failed: could not store the card", started);
        }
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
