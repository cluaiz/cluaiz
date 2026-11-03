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
//! (the caller asks). Nothing becomes memory on its own, and nothing is replaced by guessing: a
//! fact replaces an older one only when the older one is named (`replaces`), and then the older
//! file is rewritten in place with the new text (one file per subject; git keeps the history of a
//! project in git). [`similar`] finds the facts that might be about the same thing, so the model
//! or the user can decide first. `forget` deletes a fact's file. Files an older Engine marked
//! `status: superseded` stay out of the index.

use std::path::PathBuf;

use cluaiz_core::prompts;

use super::{split_front_matter, Kind, Workspace};

/// The name of the tool the model saves a fact with
pub const REMEMBER: &str = "remember";
/// The name of the tool the model deletes a wrong or outdated fact with
pub const FORGET: &str = "forget";
/// Facts shown to the model as "maybe the same thing" before it saves one
pub const SIMILAR_SHOWN: usize = 3;
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

/// The live facts that may be about the same thing as a new fact: the same name first, then by
/// word overlap ([`crate::memory::notes::SIMILAR_WORDS`]), at most `limit`
pub fn similar<'a>(workspace: &'a Workspace, name: &str, fact: &str, limit: usize) -> Vec<&'a super::Item> {
    let name = slug(name);
    let mut found: Vec<(&super::Item, f32)> = workspace
        .memory
        .iter()
        .filter(|old| old.superseded_by.is_none())
        .filter_map(|old| {
            let score = if old.name.eq_ignore_ascii_case(&name) {
                2.0
            } else {
                old.body().map(|body| crate::memory::notes::word_overlap(&body, fact)).unwrap_or(0.0)
            };
            (score >= crate::memory::notes::SIMILAR_WORDS).then_some((old, score))
        })
        .collect();
    found.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    found.into_iter().take(limit).map(|(item, _)| item).collect()
}

/// The live fact named `name`
pub fn live<'a>(workspace: &'a Workspace, name: &str) -> Option<&'a super::Item> {
    let name = name.trim().trim_end_matches(".md");
    workspace.memory.iter().find(|item| item.superseded_by.is_none() && (item.name.eq_ignore_ascii_case(name) || item.name.eq_ignore_ascii_case(&slug(name))))
}

/// Writes a confirmed fact (the user said yes). With `replaces`, the live fact of that name gets
/// the new text in its own file (it keeps its name and `created`); without, a new file.
pub fn save(workspace: &Workspace, name: &str, fact: &str, source: &str, replaces: Option<&str>) -> Result<Saved, String> {
    let (name, fact) = prepare(name, fact)?;
    let old = match replaces {
        Some(target) => Some(live(workspace, target).ok_or_else(|| format!("there is no fact named {target} to replace"))?),
        None => None,
    };
    let dir = folder(workspace);
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let path = old.map(|item| item.path.clone()).unwrap_or_else(|| path_for(workspace, &name));
    let file_name = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or(name);
    let description: String = fact.lines().next().unwrap_or_default().trim().chars().take(super::DESCRIPTION_CHARS).collect();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let created = old
        .and_then(|item| std::fs::read_to_string(&item.path).ok())
        .and_then(|text| split_front_matter(&text).0.get("created").and_then(|v| v.as_str()).map(str::to_owned))
        .unwrap_or_else(|| now.clone());
    let mut front = serde_yaml::Mapping::new();
    let mut put = |key: &str, value: &str| {
        front.insert(key.into(), value.into());
    };
    put("name", &file_name);
    put("description", &description);
    put("status", "confirmed");
    put("source", source);
    put("created", &created);
    if old.is_some() {
        put("updated", &now);
    }
    write(&path, &front, &fact)?;
    Ok(Saved { name: file_name, path, replaced: old.map(|item| vec![item.name.clone()]).unwrap_or_default() })
}

/// Deletes the live fact named `name` (the user said yes). Returns its file.
pub fn forget(workspace: &Workspace, name: &str) -> Result<PathBuf, String> {
    let item = live(workspace, name).ok_or_else(|| format!("there is no fact named {name}"))?;
    std::fs::remove_file(&item.path).map_err(|error| format!("{}: {error}", item.path.display()))?;
    Ok(item.path.clone())
}

/// What a model is told when a new fact may repeat facts that are there: their names and texts,
/// and how to go on
pub fn similar_text(items: &[&super::Item]) -> String {
    let lines: Vec<String> = items.iter().map(|item| format!("- {}: {}", item.name, item.body().unwrap_or_default().trim())).collect();
    prompts::render("workspace.remember_similar", &[("facts", &lines.join("\n"))])
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
                    "fact": { "type": "string", "description": "The fact, in one or two sentences that make sense on their own later" },
                    "replaces": { "type": "string", "description": "The name of the fact this one corrects or updates, or \"none\" when it is a separate fact" }
                },
                "required": ["name", "fact"]
            }
        }
    })
}

/// The `forget` schema
pub fn forget_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": FORGET,
            "description": prompts::text("workspace.forget"),
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The name of the fact that is wrong or no longer holds" }
                },
                "required": ["name"]
            }
        }
    })
}
