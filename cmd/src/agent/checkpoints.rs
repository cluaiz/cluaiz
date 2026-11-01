//! What each request changed. The first content of every file a request writes or edits is kept
//! (as bytes, so a binary file is put back too), so the end of the request can say which files
//! changed (`+3 −1`, `new`), `/undo` can put back the last request's files, and `/rewind` can go
//! back to before any earlier request. Every checkpoint is saved next to the conversation
//! (`~/.cluaiz/sessions/<folder>/<session>.checkpoints/<n>/`), so it survives closing the agent and
//! comes back with `--continue` / `--resume`. Commands are not tracked: a file a command changed is
//! outside this record.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Checkpoints kept per conversation; older ones are dropped
const KEEP: usize = 50;
/// A file larger than this is not kept (its change cannot be undone; the summary says so)
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Saved checkpoints of a folder's conversations older than this are removed
const KEEP_DAYS: u64 = 30;

#[derive(Default)]
pub struct Checkpoints {
    /// Where this conversation's checkpoints are saved (None: only in memory)
    dir: Option<PathBuf>,
    /// This request: each file once, with its content before the first change (None: it was new)
    current: Vec<(PathBuf, Option<Vec<u8>>)>,
    /// Files this request changed that were too large to keep
    too_large: Vec<PathBuf>,
    /// The request's first line and the conversation length before it (for /rewind)
    title: String,
    messages_before: usize,
    /// Requests that changed files, oldest first
    done: Vec<Checkpoint>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Checkpoint {
    /// 1, 2, 3… within the conversation
    number: usize,
    title: String,
    time: String,
    /// Messages in the conversation before this request
    messages_before: usize,
    files: Vec<SavedFile>,
    #[serde(skip)]
    contents: Vec<Option<Vec<u8>>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedFile {
    path: PathBuf,
    /// The kept content's file in the checkpoint folder; None: the file was new
    backup: Option<String>,
}

impl Checkpoints {
    /// The checkpoints of one conversation, with the ones saved before (after --continue)
    pub fn open(dir: PathBuf) -> Self {
        if let Some(parent) = dir.parent() {
            remove_old(parent);
        }
        let mut done: Vec<Checkpoint> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| load(&entry.path()))
            .collect();
        done.sort_by_key(|checkpoint| checkpoint.number);
        Self { dir: Some(dir), done, ..Self::default() }
    }

    /// Called when a request starts
    pub fn begin(&mut self, request: &str, messages_before: usize) {
        self.title = request.lines().next().unwrap_or_default().chars().take(60).collect();
        self.messages_before = messages_before;
    }

    /// Called before a file is written: keeps its content the first time in this request
    pub fn before_write(&mut self, file: &Path) {
        if self.current.iter().any(|(path, _)| path == file) || self.too_large.iter().any(|path| path == file) {
            return;
        }
        if std::fs::metadata(file).is_ok_and(|meta| meta.len() > MAX_FILE_BYTES) {
            self.too_large.push(file.to_path_buf());
            return;
        }
        let original = std::fs::read(file).ok();
        self.current.push((file.to_path_buf(), original));
    }

    /// Ends the request: one line per changed file, and the checkpoint kept for /undo and /rewind
    pub fn finish(&mut self, show: impl Fn(&Path) -> String) -> Option<String> {
        let too_large = std::mem::take(&mut self.too_large);
        if self.current.is_empty() && too_large.is_empty() {
            return None;
        }
        let mut lines: Vec<String> = self
            .current
            .iter()
            .map(|(path, before)| {
                let now = std::fs::read(path).ok();
                let what = match (before, &now) {
                    (None, Some(now)) => format!("new, {} lines", String::from_utf8_lossy(now).lines().count()),
                    (Some(_), None) => "deleted".to_owned(),
                    (Some(before), Some(now)) => {
                        let (added, removed) =
                            super::line_diff::count(&String::from_utf8_lossy(before), &String::from_utf8_lossy(now));
                        format!("+{added} −{removed}")
                    }
                    (None, None) => "not created".to_owned(),
                };
                format!("{} ({what})", show(path))
            })
            .collect();
        lines.extend(too_large.iter().map(|path| format!("{} (over 10 MB: not kept, /undo cannot restore it)", show(path))));
        let current = std::mem::take(&mut self.current);
        if !current.is_empty() {
            let checkpoint = Checkpoint {
                number: self.done.last().map_or(1, |last| last.number + 1),
                title: std::mem::take(&mut self.title),
                time: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
                messages_before: self.messages_before,
                files: current
                    .iter()
                    .enumerate()
                    .map(|(i, (path, before))| SavedFile { path: path.clone(), backup: before.as_ref().map(|_| format!("{i}.bak")) })
                    .collect(),
                contents: current.into_iter().map(|(_, before)| before).collect(),
            };
            self.save(&checkpoint);
            self.done.push(checkpoint);
            while self.done.len() > KEEP {
                let oldest = self.done.remove(0);
                self.forget(&oldest);
            }
        }
        Some(format!("Changed: {}", lines.join(", ")))
    }

    /// Puts the files of the last request that changed files back; returns what was restored
    pub fn undo(&mut self, show: impl Fn(&Path) -> String) -> Result<String, String> {
        let Some(checkpoint) = self.done.pop() else {
            return Err("Nothing to undo: no request of this conversation changed files.".into());
        };
        match self.restore(&checkpoint, &show) {
            Ok(restored) => Ok(format!("Restored: {}", restored.join(", "))),
            Err(error) => {
                self.done.push(checkpoint);
                Err(error)
            }
        }
    }

    /// Puts every file back as it was before checkpoint `number` (that request and all later ones
    /// are undone, newest first); returns what was restored and the conversation length before it
    pub fn rewind(&mut self, number: usize, show: impl Fn(&Path) -> String) -> Result<(String, usize), String> {
        let Some(at) = self.done.iter().position(|checkpoint| checkpoint.number == number) else {
            return Err(format!("There is no checkpoint {number}; /rewind lists them."));
        };
        let messages_before = self.done[at].messages_before;
        let mut restored: Vec<String> = Vec::new();
        while self.done.len() > at {
            let checkpoint = self.done.pop().expect("checked above");
            let files = match self.restore(&checkpoint, &show) {
                Ok(files) => files,
                Err(error) => {
                    self.done.push(checkpoint);
                    return Err(error);
                }
            };
            for file in files {
                if !restored.contains(&file) {
                    restored.push(file);
                }
            }
        }
        Ok((format!("Restored: {}", restored.join(", ")), messages_before))
    }

    /// What /rewind shows: the checkpoints, newest first
    pub fn list(&self, show: impl Fn(&Path) -> String) -> String {
        if self.done.is_empty() {
            return "No checkpoints: no request of this conversation changed files.".into();
        }
        let mut lines = vec!["Checkpoints (/rewind <n> puts the files and the conversation back to before request n):".to_string()];
        for checkpoint in self.done.iter().rev() {
            let files: Vec<String> = checkpoint.files.iter().map(|file| show(&file.path)).collect();
            lines.push(format!("  {:>3}  {}  {}  ·  {}", checkpoint.number, checkpoint.time, checkpoint.title, files.join(", ")));
        }
        lines.join("\n")
    }

    /// Files of the last request that changed files
    pub fn last_files(&self) -> Vec<PathBuf> {
        self.done.last().map(|checkpoint| checkpoint.files.iter().map(|file| file.path.clone()).collect()).unwrap_or_default()
    }

    /// Writes the kept contents back (deletes the files that were new) and forgets the checkpoint
    fn restore(&self, checkpoint: &Checkpoint, show: &impl Fn(&Path) -> String) -> Result<Vec<String>, String> {
        let mut done = Vec::new();
        for (i, file) in checkpoint.files.iter().enumerate().rev() {
            let content = match checkpoint.contents.get(i) {
                Some(content) => content.clone(),
                None => None,
            };
            match (&file.backup, content) {
                (Some(_), Some(bytes)) => std::fs::write(&file.path, bytes).map_err(|e| format!("{}: {e}", show(&file.path)))?,
                (Some(backup), None) => {
                    let saved = self.dir.as_ref().map(|dir| dir.join(checkpoint.number.to_string()).join(backup));
                    let bytes = saved
                        .and_then(|path| std::fs::read(path).ok())
                        .ok_or_else(|| format!("{}: the saved copy is missing", show(&file.path)))?;
                    std::fs::write(&file.path, bytes).map_err(|e| format!("{}: {e}", show(&file.path)))?;
                }
                (None, _) => {
                    if file.path.exists() {
                        std::fs::remove_file(&file.path).map_err(|e| format!("{}: {e}", show(&file.path)))?;
                    }
                }
            }
            done.push(show(&file.path));
        }
        self.forget(checkpoint);
        Ok(done)
    }

    /// Saves a checkpoint: `<n>/checkpoint.json` and one `<i>.bak` per kept file
    fn save(&self, checkpoint: &Checkpoint) {
        let Some(dir) = &self.dir else { return };
        let folder = dir.join(checkpoint.number.to_string());
        let written = std::fs::create_dir_all(&folder).and_then(|_| {
            for (file, content) in checkpoint.files.iter().zip(&checkpoint.contents) {
                if let (Some(backup), Some(bytes)) = (&file.backup, content) {
                    std::fs::write(folder.join(backup), bytes)?;
                }
            }
            let json = serde_json::to_string_pretty(checkpoint).map_err(std::io::Error::other)?;
            std::fs::write(folder.join("checkpoint.json"), json)
        });
        if let Err(error) = written {
            tracing::warn!("checkpoint {} not saved: {error}", folder.display());
        }
    }

    fn forget(&self, checkpoint: &Checkpoint) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir_all(dir.join(checkpoint.number.to_string()));
        }
    }
}

/// A saved checkpoint folder (its contents stay on disk until it is restored)
fn load(folder: &Path) -> Option<Checkpoint> {
    let text = std::fs::read_to_string(folder.join("checkpoint.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Removes the checkpoint folders of this folder's conversations not touched for KEEP_DAYS
fn remove_old(sessions: &Path) {
    let limit = std::time::Duration::from_secs(KEEP_DAYS * 24 * 3600);
    for entry in std::fs::read_dir(sessions).into_iter().flatten().flatten() {
        let path = entry.path();
        let is_checkpoints = path.extension().is_some_and(|ext| ext == "checkpoints") && path.is_dir();
        let old = entry.metadata().and_then(|meta| meta.modified()).ok().and_then(|time| time.elapsed().ok()).is_some_and(|age| age > limit);
        if is_checkpoints && old {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-checkpoints-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn show(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn changes_are_counted_and_undone() {
        let dir = temp("undo");
        let old = dir.join("calc.py");
        let new = dir.join("new.py");
        std::fs::write(&old, "a\nb + 1\nc\n").unwrap();

        let mut checkpoints = Checkpoints::default();
        checkpoints.begin("fix calc", 0);
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

    #[test]
    fn checkpoints_survive_a_restart_and_rewind_several_requests() {
        let work = temp("rewind-work");
        let saved = temp("rewind-saved").join("20261101-100000.checkpoints");
        let file = work.join("app.js");
        // A file that is not UTF-8 is put back byte for byte
        let image = work.join("logo.bin");
        std::fs::write(&file, "v1").unwrap();
        std::fs::write(&image, [0u8, 159, 146, 150]).unwrap();

        let mut checkpoints = Checkpoints::open(saved.clone());
        checkpoints.begin("first change", 2);
        checkpoints.before_write(&file);
        checkpoints.before_write(&image);
        std::fs::write(&file, "v2").unwrap();
        std::fs::write(&image, b"png").unwrap();
        checkpoints.finish(show);
        checkpoints.begin("second change", 6);
        checkpoints.before_write(&file);
        std::fs::write(&file, "v3").unwrap();
        checkpoints.finish(show);
        drop(checkpoints);

        // The agent was closed: the checkpoints come back from disk
        let mut reopened = Checkpoints::open(saved.clone());
        let list = reopened.list(show);
        assert!(list.find("second change").unwrap() < list.find("first change").unwrap(), "newest first");
        let (restored, messages_before) = reopened.rewind(1, show).unwrap();
        assert_eq!(messages_before, 2);
        assert!(restored.contains("app.js") && restored.contains("logo.bin"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1");
        assert_eq!(std::fs::read(&image).unwrap(), [0u8, 159, 146, 150]);
        assert!(reopened.undo(show).is_err(), "everything was rewound");
        assert!(std::fs::read_dir(&saved).unwrap().next().is_none(), "restored checkpoints are removed from disk");
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::remove_dir_all(saved.parent().unwrap());
    }
}
