//! Durable notes (architecture §11.8): facts and preferences that outlive a chat.
//!
//! The notes live in `<CLUAIZ_HOME>/memory/notes/notes.jsonl`, one JSON line per note, and the
//! file always holds the current notes only: a change rewrites it. A note is `proposed` (by the
//! model through `remember`, waiting for the user), `confirmed` or `active`.
//!
//! Nothing is replaced by guessing. A newer note replaces an older one only when the model or the
//! user names the older one ([`NoteStore::replace`]); [`similar`] finds the notes that might be
//! about the same thing, so they can be shown first. A replaced or forgotten note leaves the file
//! at once, or, when `history_days` is set, stays there as `superseded` / `invalidated` (never
//! shown to the model) for that many days. Scope: one chat, a workspace, or every chat.

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
    /// proposed, confirmed, active; kept for `history_days` only: superseded, invalidated, or
    /// moved (now a fact of the project's `.agents/memory/`, see `moved_to`)
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
    /// The project memory file the note became (status moved)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_to: Option<String>,
    /// A proposed change of another note: once confirmed, this note replaces it (or, with
    /// `forget`, the other note is removed and this proposal goes away)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    /// A proposal to forget the note `replaces` names (`text` is that note's text)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forget: bool,
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

    fn is_dead(&self) -> bool {
        matches!(self.status.as_str(), "superseded" | "invalidated" | "moved")
    }
}

/// How a proposal names a fact of a project's memory (`fact:<name>`) instead of a note
pub const FACT_PREFIX: &str = "fact:";

/// A note id as the model or a client may write it: `n:7`, `n7` or `7`
pub fn normalize_id(id: &str) -> String {
    let id = id.trim();
    let number = id.strip_prefix("n:").or_else(|| id.strip_prefix('n')).unwrap_or(id);
    format!("n:{}", number.trim())
}

/// Words of a note's subject (for "the same subject" checks)
fn subject(text: &str) -> HashSet<String> {
    cluaiz_context::analyze::terms(text).into_iter().filter(|t| !t.chars().all(|c| c.is_ascii_digit())).collect()
}

/// How much two texts share, by words: the share of the smaller one's words found in the other
/// (0 to 1). Words, not meaning: "port 9090" and "server runs on 8080" share nothing.
pub fn word_overlap(a: &str, b: &str) -> f32 {
    let (a, b) = (subject(a), subject(b));
    let (small, large) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if small.is_empty() {
        return 0.0;
    }
    let shared = small.iter().filter(|word| large.iter().any(|other| same_word(word, other))).count();
    shared as f32 / small.len().max(2) as f32
}

/// Two notes are about the same subject when most of the smaller one's words are shared
pub fn same_subject(a: &str, b: &str) -> bool {
    word_overlap(a, b) >= 0.5
}

/// The same word, or one form of it: the shorter (at least 4 letters) starts the longer, which adds
/// at most 2 letters ("sunday" / "sundays", "deploy" / "deployed"). Letters, not a word list, so it
/// works the same in any language.
fn same_word(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    short.len() >= 4 && long.len() - short.len() <= 2 && long.starts_with(short)
}

/// Word overlap from which a note is shown as "maybe the same thing" (a third of the words): the
/// model or user decides, so a candidate too many costs a line, a missed one a duplicate
pub const SIMILAR_WORDS: f32 = 1.0 / 3.0;

/// The live notes among `notes` that may be about the same thing as `text`, most alike first, with
/// their word overlap. Only notes of `scope` and `owner` (the ones a new note there could replace).
pub fn similar<'a>(notes: &'a [Note], text: &str, scope: &str, owner: Option<&str>, limit: usize) -> Vec<(&'a Note, f32)> {
    let mut found: Vec<(&Note, f32)> = notes
        .iter()
        .filter(|n| n.is_live() && n.scope == scope && n.owner.as_deref() == owner)
        .map(|n| (n, word_overlap(&n.text, text)))
        .filter(|(_, score)| *score >= SIMILAR_WORDS)
        .collect();
    found.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    found.truncate(limit);
    found
}

/// Current state of every note from the file's lines (the newest line of a note wins, so a file
/// written as a log before reads the same)
pub fn fold(lines: &[Note]) -> Vec<Note> {
    let mut latest: BTreeMap<u64, Note> = BTreeMap::new();
    for note in lines {
        let number = note.id.strip_prefix("n:").and_then(|n| n.parse().ok()).unwrap_or(0);
        latest.insert(number, note.clone());
    }
    latest.into_values().collect()
}

/// What a change did
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The new or changed note (a proposal while it waits for the user)
    pub note: Note,
    /// The note it replaced, now superseded
    pub superseded: Option<Note>,
    /// The note it forgot, now invalidated
    pub forgotten: Option<Note>,
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

/// Days a superseded / invalidated note is kept, from settings (0: removed at once)
pub fn history_days() -> u32 {
    cluaiz_core::Settings::current().context_runtime.notes_history_days
}

impl NoteStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    pub fn shared() -> &'static NoteStore {
        &NOTES
    }

    fn read(&self) -> Vec<Note> {
        let lines: Vec<Note> = std::fs::read_to_string(&self.path)
            .map(|text| text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
            .unwrap_or_default();
        fold(&lines)
    }

    /// Writes the notes as the whole file (through a temporary file, so a crash never leaves half
    /// of it), first dropping dead notes older than `history_days`
    fn write(&self, notes: &mut Vec<Note>, history_days: u32) -> anyhow::Result<()> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(history_days as i64);
        notes.retain(|n| {
            !n.is_dead()
                || history_days > 0
                    && n.invalid_at.as_deref().or(Some(n.updated_at.as_str())).and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).is_some_and(|t| t > cutoff)
        });
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut buffer = Vec::new();
        for note in notes.iter() {
            serde_json::to_writer(&mut buffer, note)?;
            buffer.push(b'\n');
        }
        let highest = self.highest(notes);
        let temporary = self.path.with_extension("jsonl.tmp");
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&buffer)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &self.path)?;
        std::fs::write(self.seq_path(), highest.to_string())?;
        Ok(())
    }

    /// The highest note number ever given, kept beside the notes (`notes.seq`): a deleted note's
    /// id is never given again, so an id an app or a chat holds never names a different note
    fn seq_path(&self) -> PathBuf {
        self.path.with_extension("seq")
    }

    fn highest(&self, notes: &[Note]) -> u64 {
        let kept = std::fs::read_to_string(self.seq_path()).ok().and_then(|t| t.trim().parse::<u64>().ok()).unwrap_or(0);
        notes.iter().filter_map(|n| n.id.strip_prefix("n:")?.parse::<u64>().ok()).max().unwrap_or(0).max(kept)
    }

    fn next_id(&self, notes: &[Note]) -> String {
        format!("n:{}", self.highest(notes) + 1)
    }

    /// Every note in its current state
    pub fn all(&self) -> Vec<Note> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.read()
    }

    /// Adds a note. It replaces nothing: [`similar`] shows what it might duplicate, and
    /// [`NoteStore::replace`] replaces a named note.
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
        history_days: u32,
    ) -> anyhow::Result<Note> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let time = now();
        let note = Note {
            id: self.next_id(&notes),
            scope: scope.to_string(),
            owner: owner.map(str::to_string),
            text: text.trim().to_string(),
            status: status.to_string(),
            pinned,
            valid_from: time.clone(),
            invalid_at: None,
            supersedes: None,
            sources,
            by: by.to_string(),
            moved_to: None,
            replaces: None,
            forget: false,
            updated_at: time,
        };
        notes.push(note.clone());
        self.write(&mut notes, history_days)?;
        Ok(note)
    }

    /// A newer text for the live note `id` (same scope and owner, pin kept). Confirmed at once,
    /// it takes the old note's place and the old note leaves (or is kept as superseded for
    /// `history_days`); `proposed`, it waits for the user and changes nothing yet. None when
    /// there is no live note `id`.
    #[allow(clippy::too_many_arguments)]
    pub fn replace(&self, id: &str, text: &str, status: &str, by: &str, sources: Vec<String>, history_days: u32) -> anyhow::Result<Option<Change>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let id = normalize_id(id);
        let Some(old) = notes.iter().find(|n| n.id == id && n.is_live()).cloned() else { return Ok(None) };
        let time = now();
        let mut note = Note {
            id: self.next_id(&notes),
            text: text.trim().to_string(),
            status: status.to_string(),
            valid_from: time.clone(),
            invalid_at: None,
            supersedes: None,
            sources,
            by: by.to_string(),
            moved_to: None,
            replaces: Some(old.id.clone()),
            forget: false,
            updated_at: time.clone(),
            ..old.clone()
        };
        let mut superseded = None;
        if status != "proposed" {
            note.replaces = None;
            note.supersedes = Some(old.id.clone());
            superseded = Some(Self::retire(&mut notes, &old.id, "superseded", &time));
        }
        notes.push(note.clone());
        self.write(&mut notes, history_days)?;
        Ok(Some(Change { note, superseded, forgotten: None }))
    }

    /// Forgets the live note `id`: confirmed at once, it leaves (or is kept as invalidated for
    /// `history_days`); `proposed`, a proposal to forget it waits for the user. None when there is
    /// no live note `id`.
    pub fn forget(&self, id: &str, status: &str, by: &str, history_days: u32) -> anyhow::Result<Option<Change>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let id = normalize_id(id);
        let Some(old) = notes.iter().find(|n| n.id == id && n.is_live()).cloned() else { return Ok(None) };
        let time = now();
        if status == "proposed" {
            let proposal = Note {
                id: self.next_id(&notes),
                status: "proposed".into(),
                valid_from: time.clone(),
                invalid_at: None,
                supersedes: None,
                sources: Vec::new(),
                by: by.to_string(),
                moved_to: None,
                replaces: Some(old.id.clone()),
                forget: true,
                updated_at: time,
                ..old
            };
            notes.push(proposal.clone());
            self.write(&mut notes, history_days)?;
            return Ok(Some(Change { note: proposal, superseded: None, forgotten: None }));
        }
        let forgotten = Self::retire(&mut notes, &old.id, "invalidated", &time);
        self.write(&mut notes, history_days)?;
        Ok(Some(Change { note: forgotten.clone(), superseded: None, forgotten: Some(forgotten) }))
    }

    /// A proposal about a fact of a project's memory (`target` is `fact:<name>`): to replace it
    /// with `text`, or (`forget`) to delete it. Confirmed, the caller does the file work
    /// ([`crate::workspace::memory`]) and marks the proposal moved.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_for_fact(&self, text: &str, scope: &str, owner: Option<&str>, target: &str, forget: bool, sources: Vec<String>, history_days: u32) -> anyhow::Result<Note> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let time = now();
        let note = Note {
            id: self.next_id(&notes),
            scope: scope.to_string(),
            owner: owner.map(str::to_string),
            text: text.trim().to_string(),
            status: "proposed".into(),
            pinned: false,
            valid_from: time.clone(),
            invalid_at: None,
            supersedes: None,
            sources,
            by: "model".into(),
            moved_to: None,
            replaces: Some(target.to_string()),
            forget,
            updated_at: time,
        };
        notes.push(note.clone());
        self.write(&mut notes, history_days)?;
        Ok(note)
    }

    /// Marks note `id` superseded / invalidated (the write drops it unless history is kept)
    fn retire(notes: &mut [Note], id: &str, status: &str, time: &str) -> Note {
        let note = notes.iter_mut().find(|n| n.id == id).expect("the note is in the list");
        note.status = status.to_string();
        note.invalid_at = Some(time.to_string());
        note.updated_at = time.to_string();
        note.clone()
    }

    /// The note became the project memory file `path`: it is no longer live (the file is the fact
    /// now) and is kept as `moved` for `history_days` only
    pub fn mark_moved(&self, id: &str, path: &str, history_days: u32) -> anyhow::Result<Option<Note>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let Some(note) = notes.iter_mut().find(|n| n.id == id) else { return Ok(None) };
        note.status = "moved".into();
        note.moved_to = Some(path.to_string());
        note.updated_at = now();
        let moved = note.clone();
        self.write(&mut notes, history_days)?;
        Ok(Some(moved))
    }

    /// Changes a note's status or pin (confirm, invalidate, pin, unpin).
    /// - Confirming a proposed replacement makes it replace its note (when that one is still
    ///   live; otherwise it is simply confirmed); confirming a proposal to forget forgets its note
    ///   and the proposal goes away.
    /// - Invalidating a note forgets it; invalidating a proposal turns it down.
    pub fn update(&self, id: &str, status: Option<&str>, pinned: Option<bool>, history_days: u32) -> anyhow::Result<Option<Change>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut notes = self.read();
        let id = normalize_id(id);
        let Some(mut note) = notes.iter().find(|n| n.id == id).cloned() else { return Ok(None) };
        let time = now();
        let (mut superseded, mut forgotten) = (None, None);
        if let Some(status) = status {
            if status == "invalidated" {
                note.invalid_at = Some(time.clone());
            }
            let about_fact = note.replaces.as_deref().is_some_and(|r| r.starts_with(FACT_PREFIX));
            if note.status == "proposed" && matches!(status, "confirmed" | "active") && about_fact {
                // A project fact's change: confirmed here, done by the caller in the project
                note.status = status.to_string();
            } else if note.status == "proposed" && matches!(status, "confirmed" | "active") {
                let target = note.replaces.take().filter(|t| notes.iter().any(|n| &n.id == t && n.is_live()));
                match (target, note.forget) {
                    (Some(target), true) => {
                        forgotten = Some(Self::retire(&mut notes, &target, "invalidated", &time));
                        // The proposal itself is no note: it leaves with the write
                        note.status = "invalidated".into();
                        note.invalid_at = Some(time.clone());
                    }
                    (Some(target), false) => {
                        note.supersedes = Some(target.clone());
                        superseded = Some(Self::retire(&mut notes, &target, "superseded", &time));
                    }
                    (None, _) => {}
                }
                if !note.forget {
                    note.status = status.to_string();
                }
            } else {
                note.status = status.to_string();
            }
        }
        if let Some(pinned) = pinned {
            note.pinned = pinned;
        }
        note.updated_at = time;
        let mut reported = note.clone();
        if note.forget && forgotten.is_some() {
            // What the caller sees: the proposal was confirmed (and has done its work)
            reported.status = "confirmed".into();
            reported.invalid_at = None;
        }
        if let Some(kept) = notes.iter_mut().find(|n| n.id == note.id) {
            *kept = note;
        }
        self.write(&mut notes, history_days)?;
        Ok(Some(Change { note: reported, superseded, forgotten }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (NoteStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cluaiz-notes-test-{}", uuid::Uuid::new_v4().simple()));
        (NoteStore::new(dir.join("notes.jsonl")), dir)
    }

    fn add(notes: &NoteStore, text: &str, status: &str) -> Note {
        notes.add(text, "workspace", None, status, false, "user", vec![], 0).unwrap()
    }

    #[test]
    fn test_a_named_replacement_takes_over_and_the_old_note_leaves() {
        let (notes, dir) = store();
        let first = notes.add("Deploy server ka port 8011 hai", "workspace", None, "confirmed", true, "user", vec![], 0).unwrap();
        // Nothing is replaced by guessing: a second note on the subject is just added
        let other = add(&notes, "Deploy server ka port ab 9090 hai", "confirmed");
        assert_eq!(notes.all().len(), 2);
        assert_eq!(similar(&notes.all(), "deploy server port 7000", "workspace", None, 3).len(), 2);
        // Named: the new text takes the old note's place, the pin carries over, the old one is gone
        let change = notes.replace(&first.id, "Deploy server ka port 7000 hai", "confirmed", "model", vec![], 0).unwrap().unwrap();
        assert_eq!(change.superseded.unwrap().id, first.id);
        assert_eq!(change.note.supersedes.as_deref(), Some(first.id.as_str()));
        assert!(change.note.pinned, "the pin carries over");
        let ids: Vec<String> = notes.all().iter().map(|n| n.id.clone()).collect();
        assert_eq!(ids, vec![other.id.clone(), change.note.id.clone()]);
        // "n3" or "3" name the same note
        assert!(notes.replace("n99", "x", "confirmed", "model", vec![], 0).unwrap().is_none());
        assert!(notes.forget(&change.note.id.replace(':', ""), "confirmed", "model", 0).unwrap().is_some());
        assert_eq!(notes.all().len(), 1);
        // A deleted note's id is not given again
        notes.forget(&other.id, "confirmed", "user", 0).unwrap();
        assert!(notes.all().is_empty());
        assert_eq!(add(&notes, "Next", "confirmed").id, "n:4");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_history_days_keep_replaced_notes_out_of_sight() {
        let (notes, dir) = store();
        let first = add(&notes, "Standup is at 10", "confirmed");
        notes.replace(&first.id, "Standup is at 11", "confirmed", "user", vec![], 30).unwrap().unwrap();
        let all = notes.all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].status, "superseded");
        assert!(all[0].invalid_at.is_some());
        assert_eq!(all.iter().filter(|n| n.is_live()).count(), 1);
        // Another write with the same history keeps it; one with history off drops it
        notes.add("Lunch at 1", "workspace", None, "confirmed", false, "user", vec![], 30).unwrap();
        assert_eq!(notes.all().iter().filter(|n| n.status == "superseded").count(), 1);
        add(&notes, "Tea at 4", "confirmed");
        assert_eq!(notes.all().iter().filter(|n| n.status == "superseded").count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_proposals_wait_for_the_user() {
        let (notes, dir) = store();
        let sister = add(&notes, "Meri behen ka naam Riya hai", "confirmed");
        // A proposed replacement changes nothing until it is confirmed
        let proposal = notes.replace(&sister.id, "Meri behen ka naam Riya Sharma hai", "proposed", "model", vec!["c:m:4".into()], 0).unwrap().unwrap();
        assert_eq!(proposal.note.replaces.as_deref(), Some(sister.id.as_str()));
        assert!(proposal.superseded.is_none());
        assert_eq!(notes.all().iter().filter(|n| n.is_live()).count(), 1);
        let confirmed = notes.update(&proposal.note.id, Some("confirmed"), None, 0).unwrap().unwrap();
        assert_eq!(confirmed.note.status, "confirmed");
        assert_eq!(confirmed.note.supersedes.as_deref(), Some(sister.id.as_str()));
        assert_eq!(confirmed.superseded.unwrap().status, "superseded");
        let live: Vec<String> = notes.all().iter().filter(|n| n.is_live()).map(|n| n.text.clone()).collect();
        assert_eq!(live, vec!["Meri behen ka naam Riya Sharma hai"]);

        // A proposal to forget: confirmed, the note and the proposal are both gone
        let brother = add(&notes, "Mera bhai Delhi mein rehta hai", "confirmed");
        let ask = notes.forget(&brother.id, "proposed", "model", 0).unwrap().unwrap();
        assert!(ask.note.forget);
        assert_eq!(notes.all().iter().filter(|n| n.is_live()).count(), 2);
        let done = notes.update(&ask.note.id, Some("confirmed"), None, 0).unwrap().unwrap();
        assert_eq!(done.forgotten.unwrap().id, brother.id);
        assert_eq!(notes.all().len(), 1);

        // Turned down: the proposal goes, the note stays
        let keep = add(&notes, "Office ka wifi Cluaiz5G hai", "confirmed");
        let ask = notes.forget(&keep.id, "proposed", "model", 0).unwrap().unwrap();
        notes.update(&ask.note.id, Some("invalidated"), None, 0).unwrap().unwrap();
        assert!(notes.all().iter().any(|n| n.id == keep.id && n.is_live()));
        assert_eq!(notes.all().len(), 2);
        assert!(notes.update("n:99", Some("confirmed"), None, 0).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_an_old_log_file_reads_and_is_cleaned_on_the_next_write() {
        let (notes, dir) = store();
        let first = add(&notes, "Port 8011", "confirmed");
        // A file written as a log by an older Engine: the same note twice, the newest line wins
        let mut old = first.clone();
        old.status = "superseded".into();
        let mut text = std::fs::read_to_string(&notes.path).unwrap();
        text.push_str(&serde_json::to_string(&old).unwrap());
        text.push('\n');
        std::fs::write(&notes.path, text).unwrap();
        assert_eq!(notes.all()[0].status, "superseded");
        add(&notes, "Port 9090", "confirmed");
        assert_eq!(std::fs::read_to_string(&notes.path).unwrap().lines().count(), 1);
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
            moved_to: None,
            replaces: None,
            forget: false,
            updated_at: String::new(),
        };
        assert!(note("global", None).applies_to(None, None));
        assert!(note("workspace", Some("D:/proj")).applies_to(Some("c"), Some("D:/proj")));
        assert!(!note("workspace", Some("D:/proj")).applies_to(Some("c"), Some("E:/other")));
        assert!(note("chat", Some("c1")).applies_to(Some("c1"), None));
        assert!(!note("chat", Some("c1")).applies_to(Some("c2"), None));
        assert!(same_subject("deploy server port 8011", "deploy server port 9090"));
        assert!(!same_subject("deploy server port", "biryani recipe"));
        assert!(same_subject("standup meeting every Sunday", "standup meetings on Sundays"));
        assert!(same_subject("deploy the staging server", "deployed staging servers"));
        // A short word is not a form of a longer one, and unrelated words stay apart
        assert!(!same_subject("cat food", "catalog fees"));
        assert!(!same_subject("Sunday brunch", "Monday report"));
        assert_eq!(normalize_id("n7"), "n:7");
        assert_eq!(normalize_id(" n:7 "), "n:7");
        assert_eq!(normalize_id("7"), "n:7");
    }
}
