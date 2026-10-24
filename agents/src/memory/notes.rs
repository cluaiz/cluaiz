//! Durable notes (architecture §11.8): facts and preferences that outlive a chat.
//!
//! Every change is one JSON line in `<CLUAIZ_HOME>/memory/notes/notes.jsonl` (the newest line of
//! a note wins), so nothing is overwritten or lost. A note is `proposed` (by the model through
//! `remember`, or by a rule for "yaad rakhna …"), `confirmed` or `active`, and later `superseded`
//! (a newer note on the same subject took over) or `invalidated`. Scope: one chat, a workspace,
//! or every chat.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `n:<number>`
    pub id: String,
    /// chat, workspace or global
    pub scope: String,
    /// The chat (scope chat) or the workspace folder (scope workspace)
    #[serde(default)]
    pub owner: Option<String>,
    pub text: String,
    /// proposed, confirmed, active, superseded, invalidated
    pub status: String,
    #[serde(default)]
    pub pinned: bool,
    pub valid_from: String,
    #[serde(default)]
    pub invalid_at: Option<String>,
    /// The note this one replaced
    #[serde(default)]
    pub supersedes: Option<String>,
    /// Where it came from (`<chat>:m:<pos>`)
    #[serde(default)]
    pub sources: Vec<String>,
    /// user, model, rule
    pub by: String,
    pub updated_at: String,
}

impl Note {
    pub fn is_live(&self) -> bool {
        matches!(self.status.as_str(), "confirmed" | "active")
    }

    /// Whether the note applies to a chat in a workspace
    pub fn applies_to(&self, chat: Option<&str>, workspace: Option<&str>) -> bool {
        match self.scope.as_str() {
            "global" => true,
            "workspace" => self.owner.is_none() || self.owner.as_deref() == workspace,
            "chat" => chat.is_some() && self.owner.as_deref() == chat,
            _ => false,
        }
    }
}

/// Words of a note's subject (for "the same subject" checks)
fn subject(text: &str) -> HashSet<String> {
    cluaiz_context::analyze::terms(text).into_iter().filter(|t| !t.chars().all(|c| c.is_ascii_digit())).collect()
}

/// Two notes are about the same subject when most of the smaller one's words are shared
pub fn same_subject(a: &str, b: &str) -> bool {
    let (a, b) = (subject(a), subject(b));
    let smaller = a.len().min(b.len());
    smaller > 0 && a.intersection(&b).count() * 2 >= smaller.max(2)
}

/// Current state of every note from the log lines (the newest line of a note wins)
pub fn fold(lines: &[Note]) -> Vec<Note> {
    let mut latest: BTreeMap<u64, Note> = BTreeMap::new();
    for note in lines {
        let number = note.id.strip_prefix("n:").and_then(|n| n.parse().ok()).unwrap_or(0);
        latest.insert(number, note.clone());
    }
    latest.into_values().collect()
}

/// The notes file, with one writer at a time
pub struct NoteStore {
    path: PathBuf,
    lock: Mutex<()>,
}

static NOTES: LazyLock<NoteStore> = LazyLock::new(|| NoteStore::new(super::memory_root().join("notes").join("notes.jsonl")));

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl NoteStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    pub fn shared() -> &'static NoteStore {
        &NOTES
    }

    fn read_lines(&self) -> Vec<Note> {
        std::fs::read_to_string(&self.path)
            .map(|text| text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
            .unwrap_or_default()
    }

    fn append(&self, notes: &[Note]) -> anyhow::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        let mut buffer = Vec::new();
        for note in notes {
            serde_json::to_writer(&mut buffer, note)?;
            buffer.push(b'\n');
        }
        file.write_all(&buffer)?;
        file.sync_all()?;
        Ok(())
    }

    /// Every note in its current state
    pub fn all(&self) -> Vec<Note> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        fold(&self.read_lines())
    }

    /// Adds a note. A live note of the same scope and owner on the same subject is superseded
    /// by it (both are kept). Returns the new note and the one it superseded.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &self,
        text: &str,
        scope: &str,
        owner: Option<&str>,
        status: &str,
        pinned: bool,
        by: &str,
        sources: Vec<String>,
    ) -> anyhow::Result<(Note, Option<Note>)> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let current = fold(&self.read_lines());
        let number = current.iter().filter_map(|n| n.id.strip_prefix("n:")?.parse::<u64>().ok()).max().unwrap_or(0) + 1;
        let time = now();
        let old = current
            .iter()
            .filter(|n| n.is_live() && n.scope == scope && n.owner.as_deref() == owner && same_subject(&n.text, text))
            .max_by_key(|n| n.updated_at.clone())
            .cloned();
        let note = Note {
            id: format!("n:{number}"),
            scope: scope.to_string(),
            owner: owner.map(str::to_string),
            text: text.trim().to_string(),
            status: status.to_string(),
            pinned: pinned || old.as_ref().is_some_and(|o| o.pinned),
            valid_from: time.clone(),
            invalid_at: None,
            supersedes: None,
            sources,
            by: by.to_string(),
            updated_at: time.clone(),
        };
        // A proposed note does not replace anything until it is confirmed
        let (note, superseded) = match old.filter(|_| status != "proposed") {
            Some(old) => {
                let replaced = Note { status: "superseded".into(), invalid_at: Some(time.clone()), updated_at: time.clone(), ..old };
                (Note { supersedes: Some(replaced.id.clone()), ..note }, Some(replaced))
            }
            None => (note, None),
        };
        let mut lines = vec![note.clone()];
        lines.extend(superseded.clone());
        self.append(&lines)?;
        Ok((note, superseded))
    }

    /// Changes a note's status or pin (confirm, invalidate, pin, unpin). Confirming a proposed
    /// note supersedes a live note on the same subject.
    pub fn update(&self, id: &str, status: Option<&str>, pinned: Option<bool>) -> anyhow::Result<Option<(Note, Option<Note>)>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let current = fold(&self.read_lines());
        let Some(mut note) = current.iter().find(|n| n.id == id).cloned() else { return Ok(None) };
        let time = now();
        let mut superseded = None;
        if let Some(status) = status {
            if status == "invalidated" {
                note.invalid_at = Some(time.clone());
            }
            if note.status == "proposed" && matches!(status, "confirmed" | "active") {
                if let Some(old) = current
                    .iter()
                    .filter(|n| n.id != note.id && n.is_live() && n.scope == note.scope && n.owner == note.owner && same_subject(&n.text, &note.text))
                    .max_by_key(|n| n.updated_at.clone())
                {
                    note.supersedes = Some(old.id.clone());
                    superseded = Some(Note { status: "superseded".into(), invalid_at: Some(time.clone()), updated_at: time.clone(), ..old.clone() });
                }
            }
            note.status = status.to_string();
        }
        if let Some(pinned) = pinned {
            note.pinned = pinned;
        }
        note.updated_at = time;
        let mut lines = vec![note.clone()];
        lines.extend(superseded.clone());
        self.append(&lines)?;
        Ok(Some((note, superseded)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (NoteStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cluaiz-notes-test-{}", uuid::Uuid::new_v4().simple()));
        (NoteStore::new(dir.join("notes.jsonl")), dir)
    }

    #[test]
    fn test_a_correction_supersedes_and_both_are_kept() {
        let (notes, dir) = store();
        let (first, _) = notes.add("Deploy server ka port 8011 hai", "workspace", None, "confirmed", true, "user", vec![]).unwrap();
        let (second, replaced) = notes.add("Deploy server ka port ab 9090 hai", "workspace", None, "confirmed", false, "user", vec![]).unwrap();
        assert_eq!(replaced.unwrap().id, first.id);
        assert_eq!(second.supersedes.as_deref(), Some(first.id.as_str()));
        assert!(second.pinned, "the pin carries over");
        let all = notes.all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].status, "superseded");
        assert!(all[0].invalid_at.is_some());
        assert_eq!(all[1].status, "confirmed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_proposed_notes_wait_for_confirmation() {
        let (notes, dir) = store();
        notes.add("Meri behen ka naam Riya hai", "global", None, "confirmed", false, "user", vec![]).unwrap();
        let (proposed, replaced) = notes.add("Meri behen ka naam Riya Sharma hai", "global", None, "proposed", false, "model", vec!["c:m:4".into()]).unwrap();
        assert!(replaced.is_none());
        assert_eq!(notes.all().iter().filter(|n| n.is_live()).count(), 1);
        let (confirmed, replaced) = notes.update(&proposed.id, Some("confirmed"), None).unwrap().unwrap();
        assert_eq!(confirmed.status, "confirmed");
        assert_eq!(replaced.unwrap().status, "superseded");
        assert!(notes.update("n:99", Some("confirmed"), None).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_scopes_and_subjects() {
        let note = |scope: &str, owner: Option<&str>| Note {
            id: "n:1".into(),
            scope: scope.into(),
            owner: owner.map(str::to_string),
            text: "x".into(),
            status: "confirmed".into(),
            pinned: false,
            valid_from: String::new(),
            invalid_at: None,
            supersedes: None,
            sources: vec![],
            by: "user".into(),
            updated_at: String::new(),
        };
        assert!(note("global", None).applies_to(None, None));
        assert!(note("workspace", Some("D:/proj")).applies_to(Some("c"), Some("D:/proj")));
        assert!(!note("workspace", Some("D:/proj")).applies_to(Some("c"), Some("E:/other")));
        assert!(note("chat", Some("c1")).applies_to(Some("c1"), None));
        assert!(!note("chat", Some("c1")).applies_to(Some("c2"), None));
        assert!(same_subject("deploy server port 8011", "deploy server port 9090"));
        assert!(!same_subject("deploy server port", "biryani recipe"));
    }
}
