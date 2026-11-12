//! Values one flow keeps between runs: the paths `changed` / `previous` read, and the outputs of
//! steps with an `idempotency_key` (kept at least 24 hours). Kept apart from runs, so retention
//! does not remove them.

use crate::journal::Paths;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// How long an idempotency key is remembered (*proposed*: 24 hours)
const KEY_KEPT_MS: i64 = 24 * 3_600_000;

pub(crate) fn file(paths: &Paths, scope: &str, flow: &str, name: &str) -> PathBuf {
    paths.state_dir(scope, flow).join(name)
}

pub(crate) fn read_map(path: &PathBuf) -> Map<String, Value> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Runs and triggers of the same flow write these files one at a time
pub(crate) static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn write_map(path: &PathBuf, map: &Map<String, Value>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, serde_json::to_vec(map).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

pub fn previous(paths: &Paths, scope: &str, flow: &str) -> Map<String, Value> {
    read_map(&file(paths, scope, flow, "previous.json"))
}

/// After a succeeded run: the values its `changed` / `previous` paths had
pub fn keep_previous(paths: &Paths, scope: &str, flow: &str, watched: &[(String, Value)]) {
    if watched.is_empty() {
        return;
    }
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(paths, scope, flow, "previous.json");
    let mut map = read_map(&path);
    for (key, value) in watched {
        map.insert(key.clone(), value.clone());
    }
    write_map(&path, &map);
}

/// The output of a step that already succeeded with this key, if it is still remembered
pub fn idempotent_output(paths: &Paths, scope: &str, flow: &str, key: &str) -> Option<Value> {
    let map = read_map(&file(paths, scope, flow, "idempotency.json"));
    let entry = map.get(key)?;
    let at = entry.get("at_ms").and_then(Value::as_i64)?;
    (chrono::Utc::now().timestamp_millis() - at < KEY_KEPT_MS).then(|| entry.get("output").cloned().unwrap_or(Value::Null))
}

pub fn keep_idempotent(paths: &Paths, scope: &str, flow: &str, key: &str, output: &Value) {
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(paths, scope, flow, "idempotency.json");
    let mut map = read_map(&path);
    let now = chrono::Utc::now().timestamp_millis();
    map.retain(|_, entry| entry.get("at_ms").and_then(Value::as_i64).is_some_and(|at| now - at < KEY_KEPT_MS));
    map.insert(key.to_string(), json!({ "at_ms": now, "output": output }));
    write_map(&path, &map);
}
