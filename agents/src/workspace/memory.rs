//! A project's memory: facts the model learned while working, kept in the project so that a new
//! chat, another computer or another AI has them too.
//!
//! One fact is one file, `<project>/.agents/memory/<name>.md`:
//!
//! ```markdown
//! ---
//! name: deploy-port
//! description: The deploy server listens on port 9090
//! status: confirmed
//! source: cluaiz agent, model qwen3-4b
//! created: 2026-11-01T10:00:00Z
//! ---
//! The deploy server listens on port 9090.
//! ```
//!
//! The model proposes a fact with `remember`; the user confirms it before anything is written
//! (the caller asks). Nothing becomes memory on its own. A newer fact on the same subject (or of
//! the same name) replaces the older one: the older file stays, marked `status: superseded` and
//! `superseded_by: <name>`, and leaves the index.

use std::path::PathBuf;

use cluaiz_core::prompts;

use super::{split_front_matter, Kind, Workspace};

/// The name of the tool the model saves a fact with
pub const REMEMBER: &str = "remember";
/// Most characters of one fact: memory keeps points, not conversations
pub const MAX_FACT_CHARS: usize = 2_000;
/// Most characters of a fact's file name
const MAX_NAME_CHARS: usize = 60;

/// What `save` wrote
#[derive(Debug, Clone)]
pub struct Saved {
    pub name: String,
    pub path: PathBuf,
    /// The names of the facts it replaced
    pub replaced: Vec<String>,
}

/// A file name from what the model called the fact: lower case letters, digits and `-`
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(MAX_NAME_CHARS).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "fact".into() } else { out }
}

/// The folder new facts go to
pub fn folder(workspace: &Workspace) -> PathBuf {
    workspace.root.join(".agents").join(Kind::Memory.folder())
}

/// The file a new fact of this name goes to: always a new file, so a replaced fact keeps its own
pub fn path_for(workspace: &Workspace, name: &str) -> PathBuf {
    let dir = folder(workspace);
    let mut file_name = name.to_owned();
    let mut n = 2;
    while dir.join(format!("{file_name}.md")).exists() {
        file_name = format!("{name}-{n}");
        n += 1;
    }
    dir.join(format!("{file_name}.md"))
}

/// The fact checked as it would be saved: its file name and text, or why it cannot be saved
pub fn prepare(name: &str, fact: &str) -> Result<(String, String), String> {
    let fact = fact.trim();
    if fact.is_empty() {
        return Err("give the fact to remember".into());
    }
    if fact.chars().count() > MAX_FACT_CHARS {
        return Err(format!("a memory is one short fact; write it in at most {MAX_FACT_CHARS} characters"));
    }
    Ok((slug(name), fact.to_owned()))
}

/// The live facts a new fact would replace: the same name, or the same subject
pub fn replaces<'a>(workspace: &'a Workspace, name: &str, fact: &str) -> Vec<&'a super::Item> {
    workspace
        .memory
        .iter()
        .filter(|old| old.superseded_by.is_none())
        .filter(|old| old.name.eq_ignore_ascii_case(name) || old.body().is_ok_and(|body| crate::memory::notes::same_subject(&body, fact)))
        .collect()
}

/// Writes a confirmed fact (the user said yes) and marks the facts it replaces
pub fn save(workspace: &Workspace, name: &str, fact: &str, source: &str) -> Result<Saved, String> {
    let (name, fact) = prepare(name, fact)?;
    let old = replaces(workspace, &name, &fact);
    let dir = folder(workspace);
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let path = path_for(workspace, &name);
    let file_name = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or(name);
    let description: String = fact.lines().next().unwrap_or_default().trim().chars().take(super::DESCRIPTION_CHARS).collect();
    let mut front = serde_yaml::Mapping::new();
    let mut put = |key: &str, value: &str| {
        front.insert(key.into(), value.into());
    };
    put("name", &file_name);
    put("description", &description);
    put("status", "confirmed");
    put("source", source);
    put("created", &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let replaced: Vec<String> = old.iter().map(|item| item.name.clone()).collect();
    if !replaced.is_empty() {
        put("supersedes", &replaced.join(", "));
    }
    write(&path, &front, &fact)?;
    for item in old {
        let text = std::fs::read_to_string(&item.path).map_err(|error| format!("{}: {error}", item.path.display()))?;
        let (mut front, body) = split_front_matter(&text);
        front.insert("status".into(), "superseded".into());
        front.insert("superseded_by".into(), file_name.as_str().into());
        write(&item.path, &front, &body)?;
    }
    Ok(Saved { name: file_name, path, replaced })
}

fn write(path: &std::path::Path, front: &serde_yaml::Mapping, body: &str) -> Result<(), String> {
    let yaml = serde_yaml::to_string(front).map_err(|error| error.to_string())?;
    std::fs::write(path, format!("---\n{yaml}---\n{}\n", body.trim())).map_err(|error| format!("{}: {error}", path.display()))
}

/// The `remember` schema
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": REMEMBER,
            "description": prompts::text("workspace.remember"),
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "A short name for the subject, like deploy-port or test-command" },
                    "fact": { "type": "string", "description": "The fact, in one or two sentences that make sense on their own later" }
                },
                "required": ["name", "fact"]
            }
        }
    })
}
