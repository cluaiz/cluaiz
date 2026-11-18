//! Every chat-model load, refusal, failure and unload of this machine, one JSON line each in
//! `engine/load_history.jsonl` beside `vram_margins.json`, so the CLI and apps can show what
//! happened and how long it took. Only the newest `KEPT` records stay. Holds ids, numbers and error
//! text, never chat content.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Mutex;

/// Records kept in the file; older ones are dropped
const KEPT: usize = 500;

/// Writes and the trim run one at a time
static FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Load,
    Unload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Loaded (or unloaded)
    Ok,
    /// llama.cpp could not load it, the retry included
    Failed,
    /// The planner found no placement that fits; nothing was loaded
    Refused,
}

/// Why a load ran
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// A request asked for this model
    Requested,
    /// The first load measured this model, and the measurement allowed a better placement
    Remeasured,
    /// Less VRAM was left after the load than the plan needs; loaded one placement smaller
    StepDown,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Position in the history, counting up from 1 (kept across the trim)
    pub seq: u64,
    /// RFC 3339, when the action ended
    pub at: String,
    pub action: Option<Action>,
    pub outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Cause>,
    /// Installed model id
    pub model: String,
    /// Weight file
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_bytes: Option<u64>,
    /// llama.cpp loads made for this action: 2 when the planned placement failed and one placement
    /// smaller was tried
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u8>,
    /// Placement as loaded (`GpuOnly`, `MoE split`, …)
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
    /// Free VRAM before the load minus after it; absent when it could not be measured (no GPU, a
    /// load fallback, shared GPU memory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_taken_gb: Option<f64>,
    /// Wall time of the action
    pub ms: u64,
    /// Why the first attempt failed (the retry may still have loaded it), or why the action failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn path() -> std::path::PathBuf {
    cluaiz_core::EnvironmentManager::current().config_dir().join("load_history.jsonl")
}

/// Reads the records in the file; lines that do not parse are skipped and counted
fn read_file(path: &std::path::Path) -> (Vec<Record>, usize) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut skipped = 0;
    let records = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Record>(line).map_err(|_| skipped += 1).ok())
        .collect();
    (records, skipped)
}

/// Appends `record` with the next seq and the time now. A failed write is logged, never returned:
/// the history must not stop a load.
pub fn append(record: Record) {
    append_to(&path(), record, KEPT);
}

fn append_to(path: &std::path::Path, mut record: Record, kept: usize) {
    let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut records, _) = read_file(path);
    record.seq = records.last().map_or(1, |last| last.seq + 1);
    record.at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let written = (|| -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if records.len() < kept {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
            writeln!(file, "{}", serde_json::to_string(&record)?)?;
        } else {
            // Full: the oldest go, and the file is written whole once (atomically, by rename)
            records.push(record.clone());
            let start = records.len() - kept;
            let mut text = String::new();
            for kept_record in &records[start..] {
                text.push_str(&serde_json::to_string(kept_record)?);
                text.push('\n');
            }
            let temporary = path.with_extension("jsonl.tmp");
            std::fs::write(&temporary, text)?;
            std::fs::rename(&temporary, path)?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        tracing::warn!("⚠️ [Load history] Could not write {}: {}", path.display(), e);
    }
}

/// One page of the history, oldest first
#[derive(Debug, Serialize)]
pub struct Page {
    pub records: Vec<Record>,
    /// Records kept now (after the model filter)
    pub total: usize,
    /// Lines of the file that could not be read
    pub skipped_lines: usize,
    /// `from_seq` for the next page; absent when this page reached the end
    pub next_from_seq: Option<u64>,
}

/// Records with `seq >= from_seq`, of `model` when given, at most `limit`
pub fn read(model: Option<&str>, from_seq: u64, limit: usize) -> Page {
    read_from(&path(), model, from_seq, limit)
}

fn read_from(path: &std::path::Path, model: Option<&str>, from_seq: u64, limit: usize) -> Page {
    let (records, skipped_lines) = read_file(path);
    let of_model: Vec<Record> = records.into_iter().filter(|r| model.is_none_or(|m| r.model == m)).collect();
    let total = of_model.len();
    let records: Vec<Record> = of_model.into_iter().filter(|r| r.seq >= from_seq).take(limit).collect();
    let next_from_seq = records.last().map(|r| r.seq + 1).filter(|_| records.len() == limit);
    Page { records, total, skipped_lines, next_from_seq }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-load-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("load_history.jsonl")
    }

    fn load(model: &str) -> Record {
        Record { action: Some(Action::Load), outcome: Some(Outcome::Ok), model: model.into(), ms: 5, ..Default::default() }
    }

    #[test]
    fn records_count_up_and_page_by_model() {
        let path = temporary_file("page");
        for model in ["a", "b", "a", "a"] {
            append_to(&path, load(model), 10);
        }
        let page = read_from(&path, Some("a"), 1, 2);
        assert_eq!(page.records.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!((page.total, page.next_from_seq), (3, Some(4)));
        let rest = read_from(&path, Some("a"), 4, 2);
        assert_eq!((rest.records.len(), rest.next_from_seq), (1, None));
        assert!(rest.records[0].at.ends_with('Z'));
    }

    #[test]
    fn only_the_newest_are_kept_and_seq_goes_on() {
        let path = temporary_file("trim");
        for _ in 0..5 {
            append_to(&path, load("m"), 3);
        }
        let page = read_from(&path, None, 1, 100);
        assert_eq!(page.records.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![3, 4, 5]);
    }

    #[test]
    fn a_broken_line_is_skipped_not_fatal() {
        let path = temporary_file("broken");
        append_to(&path, load("m"), 10);
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{not json\n").unwrap();
        append_to(&path, load("m"), 10);
        let page = read_from(&path, None, 1, 100);
        assert_eq!((page.records.len(), page.skipped_lines), (2, 1));
        assert_eq!(page.records[1].seq, 2);
    }
}
