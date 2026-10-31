//! What a request changed: the first content of every file it writes or edits is kept, so the end
//! of the request can say which files changed (`+3 −1`, `new`) and `/undo` can put them back.
//! Commands are not tracked: a file a command changed is outside this record.

use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Checkpoints {
    /// This request: each file once, with its content before the first change (None: it was new)
    current: Vec<(PathBuf, Option<String>)>,
    /// The last request that changed files, for /undo
    last: Vec<(PathBuf, Option<String>)>,
}

impl Checkpoints {
    /// Called before a file is written: keeps its content the first time in this request
    pub fn before_write(&mut self, file: &Path) {
        if self.current.iter().any(|(path, _)| path == file) {
            return;
        }
        let original = std::fs::read_to_string(file).ok();
        self.current.push((file.to_path_buf(), original));
    }

    /// Ends the request: one line per changed file, and the record kept for /undo
    pub fn finish(&mut self, show: impl Fn(&Path) -> String) -> Option<String> {
        if self.current.is_empty() {
            return None;
        }
        let lines: Vec<String> = self
            .current
            .iter()
            .map(|(path, before)| {
                let now = std::fs::read_to_string(path).ok();
                let what = match (before, &now) {
                    (None, Some(now)) => format!("new, {} lines", now.lines().count()),
                    (Some(_), None) => "deleted".to_owned(),
                    (Some(before), Some(now)) => {
                        let (added, removed) = changed_lines(before, now);
                        format!("+{added} −{removed}")
                    }
                    (None, None) => "not created".to_owned(),
                };
                format!("{} ({what})", show(path))
            })
            .collect();
        self.last = std::mem::take(&mut self.current);
        Some(format!("Changed: {}", lines.join(", ")))
    }

    /// Puts the files of the last request that changed files back; returns what was restored
    pub fn undo(&mut self, show: impl Fn(&Path) -> String) -> Result<String, String> {
        if self.last.is_empty() {
            return Err("Nothing to undo: the last requests changed no files.".into());
        }
        let mut done = Vec::new();
        for (path, before) in std::mem::take(&mut self.last).into_iter().rev() {
            match before {
                Some(text) => std::fs::write(&path, text).map_err(|e| format!("{}: {e}", show(&path)))?,
                None => {
                    if path.exists() {
                        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", show(&path)))?;
                    }
                }
            }
            done.push(show(&path));
        }
        Ok(format!("Restored: {}", done.join(", ")))
    }

    /// Files of the last request that changed files
    pub fn last_files(&self) -> Vec<PathBuf> {
        self.last.iter().map(|(path, _)| path.clone()).collect()
    }
}

/// Lines added and removed between two texts: what is left after the common start and end
/// (added, removed), the same count as the change the agent showed
fn changed_lines(before: &str, now: &str) -> (usize, usize) {
    super::line_diff::count(before, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_counted_and_undone() {
        let dir = std::env::temp_dir().join(format!("cluaiz-checkpoints-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("calc.py");
        let new = dir.join("new.py");
        std::fs::write(&old, "a\nb + 1\nc\n").unwrap();
        let _ = std::fs::remove_file(&new);
        let show = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();

        let mut checkpoints = Checkpoints::default();
        checkpoints.before_write(&old);
        std::fs::write(&old, "a\nb\nc\n").unwrap();
        checkpoints.before_write(&old); // a second change keeps the first content
        checkpoints.before_write(&new);
        std::fs::write(&new, "x\ny\n").unwrap();
        assert_eq!(checkpoints.finish(show).unwrap(), "Changed: calc.py (+1 −1), new.py (new, 2 lines)");
        assert!(checkpoints.finish(show).is_none());

        assert_eq!(checkpoints.undo(show).unwrap(), "Restored: new.py, calc.py");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "a\nb + 1\nc\n");
        assert!(!new.exists());
        assert!(checkpoints.undo(show).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
