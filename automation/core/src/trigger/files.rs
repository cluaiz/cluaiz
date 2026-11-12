//! `file` triggers: which files count, and when a touched file has settled into an event
//! (docs/automation/triggers.md#file). The host reports paths the system says were touched; this
//! decides `created`, `changed` or `removed` once a file has been still for `debounce`.

use super::Spec;
use crate::units::parse_duration;
use chrono::{DateTime, Utc};
use glob::{MatchOptions, Pattern};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Hidden and temporary files never start a run unless `ignore` says otherwise
pub const DEFAULT_IGNORE: [&str; 4] = [".*", "*.tmp", "*.part", "~*"];
const DEFAULT_DEBOUNCE_MS: i64 = 2_000;

/// Names are matched without case on Windows and macOS, as their file systems do
const OPTIONS: MatchOptions = MatchOptions { case_sensitive: cfg!(target_os = "linux"), require_literal_separator: true, require_literal_leading_dot: false };

#[derive(Debug, Clone)]
pub struct Watch {
    pub root: PathBuf,
    pub recursive: bool,
    pub events: Vec<String>,
    pub debounce_ms: i64,
    glob: Pattern,
    ignore: Vec<Pattern>,
}

/// The size and time of a file, to tell a change from a touch
pub type Stamp = (u64, Option<SystemTime>);

impl Watch {
    /// `home` replaces a leading `~`
    pub fn of(spec: &Spec, home: &Path) -> Result<Watch, String> {
        let path = spec.text("path").ok_or("a file trigger needs `path`")?;
        let root = match path.strip_prefix('~') {
            Some(rest) => home.join(rest.trim_start_matches(['/', '\\'])),
            None => PathBuf::from(path),
        };
        let pattern = |text: &str| Pattern::new(text).map_err(|e| format!("`{text}` is not a file pattern: {e}"));
        let ignore = match spec.field("ignore").and_then(Value::as_array) {
            Some(list) => list.iter().filter_map(Value::as_str).map(pattern).collect::<Result<_, _>>()?,
            None => DEFAULT_IGNORE.iter().map(|g| pattern(g)).collect::<Result<_, _>>()?,
        };
        let events = match spec.field("events").and_then(Value::as_array) {
            Some(list) => list.iter().filter_map(Value::as_str).map(String::from).collect(),
            None => vec!["created".to_string()],
        };
        let debounce_ms = spec.text("debounce").and_then(|d| parse_duration(d).ok()).unwrap_or(DEFAULT_DEBOUNCE_MS).max(0);
        Ok(Watch {
            root,
            recursive: spec.field("recursive").and_then(Value::as_bool).unwrap_or(false),
            events,
            debounce_ms,
            glob: pattern(spec.text("glob").unwrap_or("*"))?,
            ignore,
        })
    }

    /// Whether a path below the folder can start a run (its name, depth and patterns)
    pub fn wants(&self, path: &Path) -> bool {
        let Ok(below) = path.strip_prefix(&self.root) else { return false };
        let parts: Vec<String> = below.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let Some(name) = parts.last() else { return false };
        if parts.len() > 1 && !self.recursive {
            return false;
        }
        if parts.iter().any(|part| self.ignore.iter().any(|g| g.matches_with(part, OPTIONS))) {
            return false;
        }
        let relative = parts.join("/");
        self.glob.matches_with(name, OPTIONS) || self.recursive && self.glob.matches_with(&relative, OPTIONS)
    }

    /// What `trigger` holds for one event
    pub fn data(path: &Path, event: &str, stamp: Option<Stamp>) -> Value {
        let text = |s: Option<&std::ffi::OsStr>| s.map(|s| s.to_string_lossy().into_owned());
        json!({
            "path": path.display().to_string(),
            "name": text(path.file_stem()),
            "extension": text(path.extension()).unwrap_or_default(),
            "event": event,
            "size": stamp.map(|(size, _)| size),
            "modified_at": stamp.and_then(|(_, time)| time).map(|t| super::time_text(DateTime::<Utc>::from(t))),
        })
    }
}

/// Files known in the folder, and touched files waiting to be still
#[derive(Debug, Default)]
pub struct Settle {
    known: HashMap<PathBuf, Stamp>,
    /// Path → when it was last touched, in milliseconds
    touched: HashMap<PathBuf, i64>,
}

impl Settle {
    /// Files already there when watching starts are not new
    pub fn new(existing: impl IntoIterator<Item = (PathBuf, Stamp)>) -> Self {
        Self { known: existing.into_iter().collect(), touched: HashMap::new() }
    }

    pub fn touch(&mut self, path: PathBuf, now_ms: i64) {
        self.touched.insert(path, now_ms);
    }

    pub fn waiting(&self) -> bool {
        !self.touched.is_empty()
    }

    /// Paths still for `debounce_ms`, as events: `stat` tells what is there now
    pub fn due(&mut self, now_ms: i64, debounce_ms: i64, stat: impl Fn(&Path) -> Option<Stamp>) -> Vec<(PathBuf, &'static str, Option<Stamp>)> {
        let ready: Vec<PathBuf> = self.touched.iter().filter(|(_, at)| now_ms - **at >= debounce_ms).map(|(p, _)| p.clone()).collect();
        let mut out = Vec::new();
        for path in ready {
            self.touched.remove(&path);
            match (stat(&path), self.known.get(&path).copied()) {
                (Some(now), None) => {
                    self.known.insert(path.clone(), now);
                    out.push((path, "created", Some(now)));
                }
                (Some(now), Some(before)) if now != before => {
                    self.known.insert(path.clone(), now);
                    out.push((path, "changed", Some(now)));
                }
                (None, Some(_)) => {
                    self.known.remove(&path);
                    out.push((path, "removed", None));
                }
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(file: Value) -> Watch {
        let spec = super::super::specs(&json!({ "on": [{ "file": file }] })).remove(0);
        Watch::of(&spec, Path::new("/home/me")).unwrap()
    }

    #[test]
    fn names_depth_and_ignored_files() {
        let w = watch(json!({ "path": "~/Downloads", "glob": "*.pdf" }));
        assert_eq!(w.root, Path::new("/home/me/Downloads"));
        assert!(w.wants(Path::new("/home/me/Downloads/a.pdf")));
        assert!(!w.wants(Path::new("/home/me/Downloads/a.txt")));
        assert!(!w.wants(Path::new("/home/me/Downloads/sub/a.pdf")), "not recursive");
        assert!(!w.wants(Path::new("/home/me/Downloads/.a.pdf")), "hidden");
        assert!(!w.wants(Path::new("/home/me/Other/a.pdf")));
        let deep = watch(json!({ "path": "/notes", "glob": "**/*.md", "recursive": true }));
        assert!(deep.wants(Path::new("/notes/a/b/c.md")));
        assert!(!deep.wants(Path::new("/notes/.git/c.md")), "inside a hidden folder");
        let all = watch(json!({ "path": "/in", "ignore": [] }));
        assert!(all.wants(Path::new("/in/.env")));
    }

    #[test]
    fn a_file_settles_into_one_event() {
        let mut settle = Settle::new([(PathBuf::from("/in/old.csv"), (10, None))]);
        settle.touch(PathBuf::from("/in/new.csv"), 0);
        settle.touch(PathBuf::from("/in/new.csv"), 1_500);
        let stat = |p: &Path| match p.to_str() {
            Some("/in/new.csv") => Some((5, None)),
            Some("/in/old.csv") => Some((12, None)),
            _ => None,
        };
        assert!(settle.due(2_000, 2_000, stat).is_empty(), "still being written");
        let events = settle.due(3_500, 2_000, stat);
        assert_eq!(events, vec![(PathBuf::from("/in/new.csv"), "created", Some((5, None)))]);
        settle.touch(PathBuf::from("/in/old.csv"), 4_000);
        settle.touch(PathBuf::from("/in/gone.tmp"), 4_000);
        settle.touch(PathBuf::from("/in/new.csv"), 4_000);
        let mut events = settle.due(6_000, 2_000, |p| if p.ends_with("new.csv") { None } else { stat(p) });
        events.sort();
        assert_eq!(events, vec![(PathBuf::from("/in/new.csv"), "removed", None), (PathBuf::from("/in/old.csv"), "changed", Some((12, None)))]);
    }
}
