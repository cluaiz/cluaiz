//! The workspace's agent files in an App / API chat: `~/.cluaiz/AGENTS.md`, the project's
//! `AGENTS.md` files, and the rules, workflows, skills and memory facts of its `.agents/` folders.
//! They are the files the terminal agent reads (`cluaiz_agents::workspace`) and go into the system
//! prompt the same way, within a share of the context (`budget_for`). Nothing is cached: the same
//! files give the same text, so the system prompt (and the KV cache) stays the same while they do.

use std::collections::HashSet;
use std::sync::Arc;

use cluaiz_agents::workspace::{budget_for, matching, Kind, Placement, Workspace, WorkspacePrompt};
use cluaiz_context::Policy;
use serde_json::{json, Value};

/// The chat's project and what of it went into the system prompt
pub struct Project {
    pub workspace: Arc<Workspace>,
    pub prompt: WorkspacePrompt,
}

/// The workspace's agent files for a chat with a context of `n_ctx` tokens. None when the policy
/// turns them off or the chat keeps nothing of the user's (a strict private chat).
pub fn for_chat(policy: &Policy, n_ctx: usize, strict_private: bool) -> Option<Project> {
    if policy.project_files == "off" || strict_private {
        return None;
    }
    let workspace = scan(&cluaiz_agents::ToolsEngine::workspace_dir());
    let prompt = workspace.prompt(budget_for(n_ctx));
    Some(Project { workspace: Arc::new(workspace), prompt })
}

impl Project {
    /// Items the model can only reach with `read_item`: listed in the index, or cut
    pub fn has_unread(&self) -> bool {
        self.prompt.parts.iter().any(|part| part.kind != "memory" && matches!(part.placement, Placement::Index | Placement::Cut))
    }

    /// What the model already has whole: in the system prompt, a memory fact (its index line is
    /// the fact), or `sent` earlier in the chat
    pub fn known(&self, sent: &[String]) -> HashSet<String> {
        let mut known: HashSet<String> = sent.iter().cloned().collect();
        for part in &self.prompt.parts {
            if part.placement == Placement::Whole || (part.kind == "memory" && part.placement == Placement::Index) {
                known.insert(format!("{}:{}", part.kind, part.name.to_ascii_lowercase()));
            }
        }
        known
    }

    /// A message that is `/<workflow> [arguments]` for a workflow of the project: the request
    /// that runs its steps (`workflow.run`)
    pub fn workflow_request(&self, message: &str) -> Option<(String, String)> {
        let command = message.trim().strip_prefix('/')?;
        let (name, arguments) = command.split_once(char::is_whitespace).unwrap_or((command, ""));
        let item = self.workspace.find(name, Some(Kind::Workflow)).filter(|_| !name.is_empty())?;
        let steps = item.body().ok()?;
        let arguments = arguments.trim();
        let request = cluaiz_core::prompts::render(
            "workflow.run",
            &[("name", item.name.as_str()), ("steps", steps.as_str()), ("arguments", if arguments.is_empty() { "nothing" } else { arguments })],
        );
        Some((matching::key(item), request))
    }
}

/// The project a workspace note belongs to: the note's workspace when the user picked it (not
/// the Cluaiz home, the workspace nobody chose) and project files are on
pub fn note_project(note: &cluaiz_agents::memory::notes::Note) -> Option<std::path::PathBuf> {
    project_for(&note.scope, note.owner.as_deref())
}

/// [`note_project`] for a note of `scope` and `owner` that is not written yet
pub fn project_for(scope: &str, owner: Option<&str>) -> Option<std::path::PathBuf> {
    if scope != "workspace" || cluaiz_core::Settings::current().context_runtime.project_files == "off" {
        return None;
    }
    let dir = std::path::PathBuf::from(owner?);
    let home = cluaiz_core::EnvironmentManager::current().local_dir;
    (dir.is_dir() && !cluaiz_agents::workspace::same_file(&dir, &home)).then_some(dir)
}

/// A confirmed workspace note becomes a fact of its project's `.agents/memory/` (the file is the
/// fact from then on; the note leaves, kept as moved for `notes_history_days` only). The fact's
/// name is its first words. A note that names a fact (`replaces: fact:<name>`) rewrites that
/// fact's file instead, or, with `forget`, deletes it.
pub fn move_note(note: &cluaiz_agents::memory::notes::Note, dir: &std::path::Path) -> Result<(cluaiz_agents::workspace::memory::Saved, cluaiz_agents::memory::notes::Note), String> {
    use cluaiz_agents::workspace::memory;
    let workspace = scan(dir);
    let fact = note.replaces.as_deref().and_then(|r| r.strip_prefix(cluaiz_agents::memory::notes::FACT_PREFIX));
    let saved = match fact {
        Some(name) if note.forget => {
            let path = memory::forget(&workspace, name)?;
            memory::Saved { name: name.to_string(), path, replaced: vec![name.to_string()] }
        }
        _ => {
            let name: String = note.text.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
            let source = match (note.by.as_str(), note.sources.first()) {
                ("user", _) | (_, None) => format!("Cluaiz App, note {} by the {}", note.id, note.by),
                (_, Some(from)) => format!("Cluaiz App, note {} from {from}", note.id),
            };
            memory::save(&workspace, &name, &note.text, &source, fact)?
        }
    };
    let path = crate::cep::roots::display(&saved.path);
    let history = cluaiz_agents::memory::notes::history_days();
    let moved = cluaiz_agents::memory::notes::NoteStore::shared().mark_moved(&note.id, &path, history).map_err(|e| e.to_string())?.ok_or("the note is gone")?;
    Ok((saved, moved))
}

/// The `read_item` result: the item's text, or why there is none. `Some(key)` names what the
/// model got whole.
pub fn read_item(workspace: &Workspace, name: &str, kind: &str) -> (String, Option<String>) {
    let kind = Some(kind).filter(|kind| !kind.is_empty());
    match workspace.read_item(name, kind) {
        Ok((_, text)) => {
            let key = workspace.find(name.trim().trim_start_matches('/'), kind.and_then(Kind::parse)).map(matching::key);
            (text, key)
        }
        Err(error) => (format!("read_item: {error}"), None),
    }
}

/// The workspace's agent files. Skills installed from the registry are left out of the index:
/// the model finds them with the tool search, like every installed tool.
pub fn scan(dir: &std::path::Path) -> Workspace {
    let installed: Vec<String> = cluaiz_agents::ToolsEngine::registry().map(|registry| registry.installed_tools.into_keys().collect()).unwrap_or_default();
    Workspace::for_folder(dir, &installed)
}

/// `context_report.plan.project`: the project root, the budget, and how each file went in
pub fn report(project: Option<&Project>) -> Value {
    let Some(project) = project else {
        return Value::Null;
    };
    json!({
        "root": crate::cep::roots::display(&project.workspace.root),
        "budget_chars": project.prompt.budget,
        "chars": project.prompt.text.len(),
        "parts": project.prompt.parts,
        "problems": project.workspace.problems,
    })
}

/// Most argument strings of one call looked at for project files
const MAX_PATHS: usize = 32;

/// The project files a call of an installed tool named: every string of its arguments (nested
/// ones too) that is a file inside the project after the call ran, as its path from the project
/// root. Relative paths are taken from `base`, the chat's workspace folder, where installed tools
/// run. No tool or argument names are known here: a string that is no file of the project is left
/// out.
pub fn files_of_call(workspace: &Workspace, base: &std::path::Path, arguments: &str) -> Vec<String> {
    let Ok(args) = serde_json::from_str::<Value>(arguments) else { return Vec::new() };
    let mut strings = Vec::new();
    collect_strings(&args, &mut strings);
    let Ok(root) = workspace.root.canonicalize() else { return Vec::new() };
    let mut files: Vec<String> = Vec::new();
    for text in strings.into_iter().take(MAX_PATHS) {
        let text = text.trim();
        if text.is_empty() || text.len() > 1024 || text.contains(['\n', '\r', '\0']) {
            continue;
        }
        let path = std::path::Path::new(text);
        let full = if path.is_absolute() { path.to_path_buf() } else { base.join(path) };
        let Ok(full) = full.canonicalize() else { continue };
        if !full.is_file() {
            continue;
        }
        let Ok(relative) = full.strip_prefix(&root) else { continue };
        let relative = relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        if !relative.is_empty() && !files.contains(&relative) {
            files.push(relative);
        }
    }
    files
}

fn collect_strings<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|item| collect_strings(item, out)),
        _ => {}
    }
}

/// The text of the `glob` rules for the project files a call named, for its result, leaving out
/// the rules in `sent` (sent in this chat already) and adding the ones it returns to it. The
/// rules sent, as `(name, file)`.
pub fn glob_rules(workspace: &Workspace, files: &[String], sent: &mut Vec<String>) -> (String, Vec<(String, String)>) {
    let mut text = String::new();
    let mut applied = Vec::new();
    for file in files {
        for rule in workspace.rules_for_file(file) {
            let key = matching::key(rule);
            if sent.contains(&key) {
                continue;
            }
            let Ok(body) = rule.body() else { continue };
            let heading = cluaiz_core::prompts::render("workspace.glob_rule", &[("name", &rule.name), ("path", file)]);
            text.push_str(&format!("\n\n{heading}\n{body}"));
            sent.push(key);
            applied.push((rule.name.clone(), file.clone()));
        }
    }
    (text, applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_call_on_a_project_file_brings_its_glob_rules_once() {
        let repo = std::env::temp_dir().join(format!("cluaiz-glob-call-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join(".agents/rules")).unwrap();
        std::fs::create_dir_all(repo.join("web/src")).unwrap();
        std::fs::write(repo.join(".agents/rules/ts.md"), "---
trigger: glob
globs: \"*.ts\"
---
Strict TypeScript.").unwrap();
        std::fs::write(repo.join(".agents/rules/docs.md"), "---
trigger: glob
globs: [\"docs/**\"]
---
English only.").unwrap();
        std::fs::write(repo.join("web/src/app.ts"), "x").unwrap();
        std::fs::write(repo.join("readme.md"), "x").unwrap();
        let workspace = Workspace::for_folder(&repo, &[]);
        let absolute = repo.join("web/src/app.ts").display().to_string();

        // Any argument, nested or not, relative to the workspace folder or absolute
        let args = json!({ "edits": [{ "file": "web/src/app.ts" }], "content": "hello", "other": "readme.md" }).to_string();
        assert_eq!(files_of_call(&workspace, &repo, &args), ["web/src/app.ts", "readme.md"]);
        assert_eq!(files_of_call(&workspace, &repo.join("web"), &json!({ "path": "src/app.ts" }).to_string()), ["web/src/app.ts"]);
        assert_eq!(files_of_call(&workspace, &repo, &json!({ "path": absolute }).to_string()), ["web/src/app.ts"]);
        // A folder, a missing file, text and a file outside the project are no project files
        let outside = std::env::temp_dir().display().to_string();
        assert!(files_of_call(&workspace, &repo, &json!({ "path": "web", "q": "missing.ts", "n": 3, "o": outside }).to_string()).is_empty());

        let mut sent = Vec::new();
        let (text, applied) = glob_rules(&workspace, &["web/src/app.ts".into(), "readme.md".into()], &mut sent);
        assert!(text.contains("Strict TypeScript.") && text.contains("web/src/app.ts"));
        assert_eq!(applied, [("ts".to_string(), "web/src/app.ts".to_string())]);
        assert_eq!(sent, ["rule:ts"]);
        // Once in a chat
        let (text, applied) = glob_rules(&workspace, &["web/src/app.ts".into()], &mut sent);
        assert!(text.is_empty() && applied.is_empty());
        let _ = std::fs::remove_dir_all(repo);
    }
}
