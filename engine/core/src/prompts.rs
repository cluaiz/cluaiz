//! Managed prompts: every instruction the Engine itself sends to a model, with an id.
//!
//! The defaults are the Markdown files in `engine/core/prompts/` (embedded at build time). A user
//! or an app replaces one with `<CLUAIZ_HOME>/prompts/<id>.md`; the replaced text is kept in
//! `<CLUAIZ_HOME>/prompts/history/<id>/` so it can be brought back. A file has front matter
//! (`name`, `description`, `variables`) and the text under it; `{{variable}}` is filled in when
//! the prompt is used. The Engine adds no prompt of its own beyond these.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// The default files; the list of ids is the list of files
const DEFAULTS: &[&str] = &[
    include_str!("../prompts/agent.system.md"),
    include_str!("../prompts/agent.init.md"),
    include_str!("../prompts/agent.compact.md"),
    include_str!("../prompts/agent.compact_system.md"),
    include_str!("../prompts/workspace.instructions.md"),
    include_str!("../prompts/workspace.rules.md"),
    include_str!("../prompts/workspace.index.md"),
    include_str!("../prompts/workspace.cut.md"),
    include_str!("../prompts/workspace.read_item.md"),
    include_str!("../prompts/workspace.remember.md"),
    include_str!("../prompts/workflow.run.md"),
    include_str!("../prompts/chat.notes_pinned.md"),
    include_str!("../prompts/chat.notes_record.md"),
    include_str!("../prompts/chat.card.md"),
    include_str!("../prompts/tools.header.md"),
    include_str!("../prompts/tools.call_format.md"),
    include_str!("../prompts/tools.workspace.md"),
    include_str!("../prompts/tools.hint.md"),
    include_str!("../prompts/tools.hint_titles.md"),
];

/// Replaced texts kept per prompt
const HISTORY_KEEP: usize = 20;
/// Longest prompt text accepted
pub const MAX_CHARS: usize = 20_000;

#[derive(Clone, Debug, Serialize)]
pub struct Prompt {
    pub id: String,
    pub description: String,
    /// The `{{name}}` values the Engine fills in
    pub variables: Vec<String>,
    pub text: String,
    /// The user's file is in use (false: the default)
    pub overridden: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntry {
    /// When it was replaced (UTC, `YYYYMMDDTHHMMSS.mmmZ`)
    pub replaced_at: String,
    pub text: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FrontMatter {
    name: String,
    description: String,
    variables: Vec<String>,
}

/// `---` front matter and the text under it. Line ends become `\n` (a checkout may turn the
/// embedded files into CRLF) and the blank lines around the text are dropped.
fn parse(file: &str) -> (FrontMatter, String) {
    let file = file.replace("\r\n", "\n");
    if let Some(rest) = file.strip_prefix("---\n") {
        if let Some((head, body)) = rest.split_once("\n---\n").or_else(|| rest.strip_suffix("\n---").map(|head| (head, ""))) {
            if let Ok(front) = serde_yaml::from_str::<FrontMatter>(head) {
                return (front, body.trim_matches('\n').to_string());
            }
        }
    }
    (FrontMatter::default(), file.trim_matches('\n').to_string())
}

static DEFAULT_PROMPTS: Lazy<Vec<Prompt>> = Lazy::new(|| {
    DEFAULTS
        .iter()
        .map(|file| {
            let (front, text) = parse(file);
            Prompt { id: front.name, description: front.description, variables: front.variables, text, overridden: false }
        })
        .collect()
});

/// Override files already read: (path, modification time) → text
static CACHE: Lazy<Mutex<HashMap<PathBuf, (SystemTime, String)>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn default_prompt(id: &str) -> Option<&'static Prompt> {
    DEFAULT_PROMPTS.iter().find(|prompt| prompt.id == id)
}

/// The prompts folder of one Cluaiz home
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(cluaiz_home: &Path) -> Self {
        Self { dir: cluaiz_home.join("prompts") }
    }

    /// The prompts of the current Cluaiz home
    pub fn current() -> Self {
        Self::new(&crate::EnvironmentManager::current().global_dir)
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }

    /// A file this store just wrote or removed is read again on its next use
    fn forget(&self, id: &str) {
        CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&self.file(id));
    }

    fn history_dir(&self, id: &str) -> PathBuf {
        self.dir.join("history").join(id)
    }

    /// Every prompt, in the order of the defaults
    pub fn list(&self) -> Vec<Prompt> {
        DEFAULT_PROMPTS.iter().filter_map(|prompt| self.get(&prompt.id)).collect()
    }

    /// The prompt in use: the user's file when there is one and it can be read, else the default.
    /// None for an unknown id.
    pub fn get(&self, id: &str) -> Option<Prompt> {
        let default = default_prompt(id)?;
        let mut prompt = default.clone();
        if let Some(text) = self.read_override(id) {
            prompt.text = text;
            prompt.overridden = true;
        }
        Some(prompt)
    }

    fn read_override(&self, id: &str) -> Option<String> {
        let path = self.file(id);
        let modified = std::fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
        let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((time, text)) = cache.get(&path) {
            if *time == modified {
                return Some(text.clone());
            }
        }
        match std::fs::read_to_string(&path) {
            Ok(file) => {
                let (_, text) = parse(&file);
                cache.insert(path, (modified, text.clone()));
                Some(text)
            }
            Err(error) => {
                tracing::warn!("prompt {id}: {} cannot be read ({error}); the default is used", path.display());
                None
            }
        }
    }

    /// The prompt's text with its variables filled in
    pub fn render(&self, id: &str, values: &[(&str, &str)]) -> String {
        let Some(prompt) = self.get(id) else {
            tracing::error!("unknown prompt id {id}");
            return String::new();
        };
        fill(&prompt.text, &prompt.variables, values)
    }

    /// Replaces the prompt's text; the text it replaces goes to the history
    pub fn set(&self, id: &str, text: &str) -> Result<Prompt, String> {
        let default = default_prompt(id).ok_or_else(|| format!("unknown prompt {id}"))?;
        let text = text.replace("\r\n", "\n").trim_matches('\n').to_string();
        if text.trim().is_empty() {
            return Err("the text is empty; use reset to go back to the default".into());
        }
        if text.chars().count() > MAX_CHARS {
            return Err(format!("the text is over {MAX_CHARS} characters"));
        }
        std::fs::create_dir_all(&self.dir).map_err(|error| format!("{}: {error}", self.dir.display()))?;
        self.keep_in_history(id)?;
        let file = format!(
            "---\nname: {id}\ndescription: {}\nvariables: [{}]\n---\n{text}\n",
            serde_json::to_string(&default.description).unwrap_or_default(),
            default.variables.join(", ")
        );
        let path = self.file(id);
        std::fs::write(&path, file).map_err(|error| format!("{}: {error}", path.display()))?;
        self.forget(id);
        self.get(id).ok_or_else(|| format!("unknown prompt {id}"))
    }

    /// Goes back to the default; the user's text goes to the history
    pub fn reset(&self, id: &str) -> Result<Prompt, String> {
        default_prompt(id).ok_or_else(|| format!("unknown prompt {id}"))?;
        self.keep_in_history(id)?;
        let path = self.file(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        self.forget(id);
        self.get(id).ok_or_else(|| format!("unknown prompt {id}"))
    }

    /// Replaced texts, newest first
    pub fn history(&self, id: &str) -> Result<Vec<HistoryEntry>, String> {
        default_prompt(id).ok_or_else(|| format!("unknown prompt {id}"))?;
        let mut entries: Vec<HistoryEntry> = std::fs::read_dir(self.history_dir(id))
            .map(|dir| {
                dir.flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().strip_suffix(".md")?.to_string();
                        let (_, text) = parse(&std::fs::read_to_string(entry.path()).ok()?);
                        Some(HistoryEntry { replaced_at: name, text })
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
        Ok(entries)
    }

    /// The user's current file, if any, into the history; only the newest entries are kept
    fn keep_in_history(&self, id: &str) -> Result<(), String> {
        let path = self.file(id);
        let Ok(current) = std::fs::read_to_string(&path) else { return Ok(()) };
        let dir = self.history_dir(id);
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        let mut stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        while dir.join(format!("{stamp}.md")).exists() {
            stamp.push('_');
        }
        std::fs::write(dir.join(format!("{stamp}.md")), current).map_err(|error| format!("{}: {error}", dir.display()))?;
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        names.sort();
        let extra = names.len().saturating_sub(HISTORY_KEEP);
        for old in names.into_iter().take(extra) {
            let _ = std::fs::remove_file(old);
        }
        Ok(())
    }
}

/// `{{name}}` of the declared variables replaced by their values; any other text stays as it is
fn fill(text: &str, variables: &[String], values: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        if variables.iter().any(|variable| variable == name) {
            out = out.replace(&format!("{{{{{name}}}}}"), value);
        }
    }
    out
}

/// The text of a prompt of the current Cluaiz home, its variables filled in
pub fn render(id: &str, values: &[(&str, &str)]) -> String {
    Store::current().render(id, values)
}

/// The text of a prompt that has no variables
pub fn text(id: &str) -> String {
    render(id, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts these prompts had as constants in the code, before they became files
    const OLD_TOOLS_HEADER: &str = "# Available Tools\nYou have access to the following tools:\n";
    const OLD_TOOLS_CALL_FORMAT: &str = "\n\nWhen you need to call a tool, you MUST output a <tool_call> XML block with a JSON object containing \"name\" and \"arguments\":\n<tool_call>\n{\"name\": \"tool_name\", \"arguments\": {\"arg_name\": \"value\"}}\n</tool_call>\nDo not output conversational filler text before or after the <tool_call> block.";
    const OLD_CARD: &str = "Write a search card for my previous message and your answer to it (not for this request). \
Reply with only one JSON object and nothing else: \
{\"title\": \"what that exchange was about, at most 10 words\", \"motive\": \"why I asked, at most 12 words\", \
\"summary\": \"at most 2 short sentences\", \"keywords\": [\"up to 6 words, in English and in my language\"]}";

    fn home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-prompts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn defaults_are_the_old_texts() {
        let store = Store::new(&home("defaults"));
        assert_eq!(store.list().len(), DEFAULTS.len());
        assert!(store.list().iter().all(|prompt| !prompt.id.is_empty() && !prompt.description.is_empty() && !prompt.overridden));
        assert_eq!(format!("{}\n", store.render("tools.header", &[])), OLD_TOOLS_HEADER);
        assert_eq!(format!("\n\n{}", store.render("tools.call_format", &[])), OLD_TOOLS_CALL_FORMAT);
        assert_eq!(store.render("chat.card", &[]), OLD_CARD);
        let system = store.render("agent.system", &[("folder", "C:/w"), ("os", "windows"), ("arch", "x86_64"), ("shell", "PowerShell")]);
        assert!(system.starts_with("You are Cluaiz Agent, a coding assistant working on the user's computer through the cluaiz command line.\nWorking folder: C:/w\nSystem: windows (x86_64); run_command uses PowerShell.\n\n- Use the tools"));
        assert!(store.get("nope").is_none());
    }

    #[test]
    fn an_override_is_used_and_reset_keeps_it_in_the_history() {
        let dir = home("override");
        let store = Store::new(&dir);
        let prompt = store.set("tools.workspace", "Work only in {{folder}}. {{other}} stays.\r\n").unwrap();
        assert!(prompt.overridden);
        assert_eq!(store.render("tools.workspace", &[("folder", "D:/p"), ("other", "x")]), "Work only in D:/p. {{other}} stays.");
        // A second change: the first text goes to the history
        store.set("tools.workspace", "Second {{folder}}").unwrap();
        assert_eq!(store.render("tools.workspace", &[("folder", "D:/p")]), "Second D:/p");
        let back = store.reset("tools.workspace").unwrap();
        assert!(!back.overridden);
        assert!(back.text.starts_with("Tools run in the project folder {{folder}}"));
        let history = store.history("tools.workspace").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].text, "Second {{folder}}");
        assert!(store.set("tools.workspace", "  \n").is_err());
        assert!(store.set("nope", "x").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_hand_written_file_without_front_matter_is_its_text() {
        let dir = home("hand");
        std::fs::create_dir_all(dir.join("prompts")).unwrap();
        std::fs::write(dir.join("prompts/tools.hint.md"), "Maybe useful, load with load_tool:\n").unwrap();
        let store = Store::new(&dir);
        assert_eq!(store.render("tools.hint", &[]), "Maybe useful, load with load_tool:");
        let _ = std::fs::remove_dir_all(dir);
    }
}
