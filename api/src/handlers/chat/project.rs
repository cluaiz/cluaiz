//! The workspace's agent files in an App / API chat: `~/.cluaiz/AGENTS.md`, the project's
//! `AGENTS.md` files, and the rules, workflows, skills and memory facts of its `.agents/` folders.
//! They are the files the terminal agent reads (`cluaiz_agents::workspace`) and go into the system
//! prompt the same way, within a share of the context (`budget_for`). Nothing is cached: the same
//! files give the same text, so the system prompt (and the KV cache) stays the same while they do.

use cluaiz_agents::workspace::{budget_for, Workspace, WorkspacePrompt};
use cluaiz_context::Policy;
use serde_json::{json, Value};

/// The chat's project and what of it went into the system prompt
pub struct Project {
    pub workspace: Workspace,
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
    Some(Project { workspace, prompt })
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
