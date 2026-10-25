//! The CEP audit log (02-security §4.7): one JSON line per write, exec or admin call and per
//! denial. The params are kept only as a hash: never file contents, tokens or chat text.

use cluaiz_cep::{Audit, AuditEntry};
use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// A file is rotated past this size; this many old files are kept (`.1` is the newest)
const MAX_BYTES: u64 = 10 * 1024 * 1024;
const KEEP: usize = 3;

pub struct AuditFile {
    path: PathBuf,
    max_bytes: u64,
    write: Mutex<()>,
}

impl AuditFile {
    pub fn new(path: PathBuf, max_bytes: u64) -> Self {
        Self { path, max_bytes, write: Mutex::new(()) }
    }

    /// `cep-audit.jsonl` next to the Engine log in the selected home
    pub fn in_home() -> Self {
        Self::new(cluaiz_core::EnvironmentManager::current().global_dir.join("cep-audit.jsonl"), MAX_BYTES)
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn rotate(&self) {
        let numbered = |n: usize| PathBuf::from(format!("{}.{n}", self.path.display()));
        let _ = std::fs::remove_file(numbered(KEEP));
        for n in (1..KEEP).rev() {
            let _ = std::fs::rename(numbered(n), numbered(n + 1));
        }
        let _ = std::fs::rename(&self.path, numbered(1));
    }

    /// The newest `limit` lines of the current file, newest first
    pub fn recent(&self, limit: usize) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        text.lines().rev().take(limit).filter_map(|line| serde_json::from_str(line).ok()).collect()
    }
}

impl Audit for AuditFile {
    fn record(&self, entry: &AuditEntry) {
        let hash: String = entry.digest.iter().map(|b| format!("{b:02x}")).collect();
        let line = json!({
            "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "principal": entry.principal,
            "method": entry.method,
            "scope": entry.scope,
            "params_sha256": hash,
            "code": entry.code,
            "ms": entry.ms,
        });
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() >= self.max_bytes) {
            self.rotate();
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let written = OpenOptions::new().create(true).append(true).open(&self.path).and_then(|mut f| writeln!(f, "{line}"));
        if let Err(e) = written {
            tracing::warn!("CEP audit line not written to {}: {e}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_cep::Scope;

    #[test]
    fn test_lines_rotation_and_no_params() {
        let dir = std::env::temp_dir().join(format!("cep-audit-{}", uuid::Uuid::new_v4().simple()));
        let audit = AuditFile::new(dir.join("cep-audit.jsonl"), 400);
        let entry = |code| AuditEntry { principal: "token:ci", method: "fs.write", scope: Some(Scope::Write), digest: [7; 32], code, ms: 3 };
        for i in 0..6 {
            audit.record(&entry(if i % 2 == 0 { None } else { Some(-32002) }));
        }
        let recent = audit.recent(10);
        assert!(!recent.is_empty());
        assert_eq!((recent[0]["method"].as_str(), recent[0]["code"].as_i64()), (Some("fs.write"), Some(-32002)));
        assert_eq!(recent[0]["params_sha256"].as_str().map(str::len), Some(64));
        assert!(dir.join("cep-audit.jsonl.1").exists(), "rotated past 400 bytes");
        assert!(!dir.join(format!("cep-audit.jsonl.{}", KEEP + 1)).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
