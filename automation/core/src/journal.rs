//! Run records on disk (docs/automation/runs.md). Each run is a folder
//! `<flows folder>/runs/<flow>/<run_id>/` with `run.json`, `journal.jsonl` (one line per event,
//! flushed before the next step starts), `flow.json` (the version the run used) and `files/`.
//! Plain files: no database.

use crate::error::FlowError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where Automation keeps runs and per-flow state
#[derive(Debug, Clone)]
pub struct Paths {
    /// `<Cluaiz folder>/flows`: the user's flows, and `runs/`, `state/` below it
    pub flows: PathBuf,
}

impl Paths {
    pub fn runs(&self) -> PathBuf {
        self.flows.join("runs")
    }
    pub fn run_dir(&self, flow: &str, run_id: &str) -> PathBuf {
        self.runs().join(flow).join(run_id)
    }
    /// Values kept between runs of one flow (`changed`/`previous`, idempotency keys)
    pub fn state_dir(&self, scope: &str, flow: &str) -> PathBuf {
        self.flows.join("state").join(scope).join(flow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn is_finished(self) -> bool {
        matches!(self, RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled)
    }
}

/// `run.json`: the run object of `flows.runs` and `flows.run_get`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    pub flow: String,
    pub scope: String,
    pub flow_hash: String,
    pub status: RunStatus,
    /// `{ id, kind, at }`; the whole trigger data is in `trigger.json`
    pub trigger: Value,
    pub inputs: Value,
    pub outputs: Value,
    pub error: Option<FlowError>,
    pub waiting_for: Value,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub running_ms: u64,
    pub steps_done: u64,
    pub task_id: Option<String>,
    pub parent: Value,
    pub rerun_of: Value,
    pub dry_run: bool,
    /// How many times the run was resumed after a restart
    #[serde(default)]
    pub attempt: u32,
    /// The folder of the flow's file when the run started (`code` modules are found there)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub started_at: String,
    pub ms: u64,
    pub error: Option<FlowError>,
}

/// One record per step that started (and per item of `each`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub id: String,
    pub place: String,
    pub kind: String,
    pub status: String,
    pub item: Value,
    pub attempts: Vec<Attempt>,
    pub input: Value,
    pub output: Value,
    pub error: Option<FlowError>,
    pub tokens: Value,
    pub model: Option<String>,
    pub detail: Value,
}

/// The key a step record is found by again after a restart: its place and its item
pub fn record_key(place: &str, item: &Value) -> String {
    if item.is_null() {
        place.to_string()
    } else {
        format!("{place}#{item}")
    }
}

pub fn now_text() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// An open run folder
pub struct RunDir {
    pub dir: PathBuf,
    journal: Mutex<std::fs::File>,
    /// Secret values: written as `***` wherever they appear
    secrets: Vec<String>,
}

/// Secrets shorter than this are not masked: they would hide ordinary words
const SHORTEST_SECRET: usize = 4;

/// The values of a flow's `secrets` (environment variables) where the run is
pub fn secret_values(flow: &Value) -> Vec<String> {
    flow.get("secrets").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).filter_map(|name| std::env::var(name).ok()).filter(|v| v.len() >= SHORTEST_SECRET).collect()
}

/// Every secret value inside strings becomes `***`
pub fn mask(value: &mut Value, secrets: &[String]) {
    if secrets.is_empty() {
        return;
    }
    match value {
        Value::String(text) => {
            for secret in secrets {
                if text.contains(secret.as_str()) {
                    *text = text.replace(secret.as_str(), "***");
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| mask(v, secrets)),
        Value::Object(map) => map.values_mut().for_each(|v| mask(v, secrets)),
        _ => {}
    }
}

impl RunDir {
    /// A new run: the folder, `flow.json` and `trigger.json`
    pub fn create(paths: &Paths, record: &RunRecord, flow: &Value, trigger: &Value) -> std::io::Result<RunDir> {
        let dir = paths.run_dir(&record.flow, &record.run_id);
        std::fs::create_dir_all(dir.join("files"))?;
        write_json(&dir.join("flow.json"), flow)?;
        write_json(&dir.join("trigger.json"), trigger)?;
        let run = Self::open(dir)?;
        run.save(record)?;
        Ok(run)
    }

    /// An existing run, to go on writing its journal
    pub fn open(dir: PathBuf) -> std::io::Result<RunDir> {
        let journal = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("journal.jsonl"))?;
        Ok(RunDir { dir, journal: Mutex::new(journal), secrets: Vec::new() })
    }

    /// Values never written to this run's files
    pub fn set_secrets(&mut self, secrets: Vec<String>) {
        self.secrets = secrets;
    }

    pub fn secrets(&self) -> &[String] {
        &self.secrets
    }

    /// Rewrites `run.json` (written to a temporary file first, so it is never half-written)
    pub fn save(&self, record: &RunRecord) -> std::io::Result<()> {
        let mut value = serde_json::to_value(record).unwrap_or_default();
        mask(&mut value, &self.secrets);
        write_json(&self.dir.join("run.json"), &value)
    }

    /// One journal line, flushed to disk before the run goes on
    pub fn append(&self, event: &str, data: Value) -> std::io::Result<()> {
        let mut line = json!({ "at": now_text(), "event": event, "data": data });
        mask(&mut line, &self.secrets);
        let mut file = self.journal.lock().unwrap_or_else(|e| e.into_inner());
        writeln!(file, "{line}")?;
        file.sync_data()
    }

    pub fn files(&self) -> PathBuf {
        self.dir.join("files")
    }
}

fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap_or_default())?;
    std::fs::rename(&temporary, path)
}

/// The proof of a run's own webhook address: made once per run, kept in its folder (not in
/// `run.json`, which readers of the run see)
pub fn hook_token(dir: &Path) -> String {
    let path = dir.join("hook.token");
    if let Ok(token) = std::fs::read_to_string(&path) {
        if token.trim().len() >= 32 {
            return token.trim().to_string();
        }
    }
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let _ = std::fs::write(&path, &token);
    token
}

pub fn read_run(dir: &Path) -> Option<RunRecord> {
    serde_json::from_slice(&std::fs::read(dir.join("run.json")).ok()?).ok()
}

pub fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// What the journal says about steps: finished records, and steps that started and never finished
#[derive(Debug, Default)]
pub struct Journal {
    pub finished: Vec<StepRecord>,
    pub unfinished: Vec<(String, Value)>,
    /// What waiting steps wrote when they started waiting (their deadline), by place and item
    pub waiting: std::collections::HashMap<String, Value>,
    /// Step id → how many records had finished when it first started
    pub started: std::collections::HashMap<String, usize>,
}

impl Journal {
    pub fn finished_by_key(&self) -> std::collections::HashMap<String, StepRecord> {
        self.finished.iter().map(|r| (record_key(&r.place, &r.item), r.clone())).collect()
    }
}

/// Reads `journal.jsonl`; a last line cut short by a crash is ignored
pub fn read_journal(dir: &Path) -> Journal {
    let mut journal = Journal::default();
    let text = std::fs::read_to_string(dir.join("journal.jsonl")).unwrap_or_default();
    let mut started: Vec<(String, Value)> = Vec::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        let data = &entry["data"];
        match entry["event"].as_str() {
            Some("step_started") => {
                if let Some(id) = data["id"].as_str() {
                    journal.started.entry(id.to_string()).or_insert(journal.finished.len());
                }
                started.push((data["place"].as_str().unwrap_or_default().to_string(), data["item"].clone()));
            }
            Some("step_waiting") => {
                let key = record_key(data["place"].as_str().unwrap_or_default(), &data["item"]);
                journal.waiting.insert(key, data.clone());
            }
            Some("step_finished") => {
                if let Ok(record) = serde_json::from_value::<StepRecord>(data.clone()) {
                    started.retain(|(place, item)| !(place == &record.place && item == &record.item));
                    journal.finished.push(record);
                }
            }
            _ => {}
        }
    }
    journal.unfinished = started;
    journal
}

/// Runs of one flow (or all), newest first
pub fn list_runs(paths: &Paths, flow: Option<&str>) -> Vec<RunRecord> {
    let runs = paths.runs();
    let flows: Vec<PathBuf> = match flow {
        Some(flow) => vec![runs.join(flow)],
        None => std::fs::read_dir(&runs).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
    };
    let mut out: Vec<RunRecord> = flows
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter_map(|entry| read_run(&entry.path()))
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.run_id.cmp(&a.run_id)));
    out
}

/// Finds a run by id (ids are unique on this machine)
pub fn find_run(paths: &Paths, run_id: &str) -> Option<PathBuf> {
    if !run_id.starts_with("run_") || run_id.contains(['/', '\\', '.']) {
        return None;
    }
    std::fs::read_dir(paths.runs()).ok()?.flatten().map(|e| e.path().join(run_id)).find(|dir| dir.join("run.json").is_file())
}

/// Removes the oldest finished runs past `keep_runs`, and finished runs older than `keep_days`
pub fn prune(paths: &Paths, flow: &str, keep_runs: usize, keep_days: i64) -> usize {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut removed = 0;
    for (n, run) in list_runs(paths, Some(flow)).into_iter().enumerate() {
        if run.status.is_finished() && (n >= keep_runs || run.created_at < cutoff) && std::fs::remove_dir_all(paths.run_dir(flow, &run.run_id)).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// A new run id: `run_` and 12 hex characters
pub fn new_run_id() -> String {
    format!("run_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}
