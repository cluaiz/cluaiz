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
