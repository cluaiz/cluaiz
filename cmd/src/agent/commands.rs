//! The existing agent commands, shared by plain help and terminal completion.

use cluaiz_terminal::app::Suggestion;

pub const COMMANDS: &[(&str, &str)] = &[
    ("/model", "Change the model"),
    ("/mode", "Security mode: Full Access, Workspace-Gated, Strict Approval (as in the App), Plan"),
    ("/think", "Thinking: auto, on, off"),
    ("/effort", "Thinking effort: auto, low, medium, high, max"),
    ("/theme", "Colours of this screen"),
    ("/clear", "Start a new conversation"),
    ("/plan", "Where the model runs: GPU / RAM blocks, context, KV cache, memory, and why"),
    ("/compact", "Replace the conversation with a summary to free context"),
    ("/undo", "Put back the files the last request changed"),
    ("/rewind", "Checkpoints of this conversation; /rewind <n> puts files and conversation back to before request n"),
    ("/diff", "git diff of the files the last request changed"),
    ("/code", "Open the folder in VS Code with the changed files (/code <path>: that file)"),
    ("/init", "The model reads this project and writes its AGENTS.md"),
    ("/memory", "AGENTS.md instructions, rules, workflows and memory facts in use (~/.cluaiz/AGENTS.md, .agents/)"),
    ("/skills", "The SKILL.md skills the model can use (.agents/skills, ~/.agents/skills)"),
    ("/rules", "Allow and deny rules in use, from ~/.cluaiz/config/agent.json, .agents/settings.json, .agents/settings.local.json"),
    ("/sessions", "Saved conversations of this folder (--continue / --resume <id>)"),
    ("!<command>", "Run a shell command yourself; the agent sees its output"),
    ("/help", "Commands and keyboard shortcuts"),
    ("/bye", "Close the agent"),
];

/// Commands that work but are not listed (other names of listed ones, and session switches)
const OTHER_NAMES: &[&str] = &["/exit", "/?", "/permissions", "/yes", "/ask", "/strict"];

/// A project workflow: `/name` and its description (from `.agents/workflows/<name>.md`)
pub type Workflow = (String, String);

/// Whether `/name` is one of the agent's own commands (a workflow of the same name is not run)
pub fn is_built_in(name: &str) -> bool {
    let command = format!("/{}", name.trim_start_matches('/'));
    COMMANDS.iter().any(|(listed, _)| listed.eq_ignore_ascii_case(&command))
        || OTHER_NAMES.iter().any(|other| other.eq_ignore_ascii_case(&command))
}

/// The project's workflows that do not share a name with a built-in command
pub fn workflows(workspace: &cluaiz_agents::workspace::Workspace) -> Vec<Workflow> {
    workspace
        .workflows
        .iter()
        .filter(|item| !is_built_in(&item.name) && !item.name.contains(char::is_whitespace))
        .map(|item| (format!("/{}", item.name), item.description.clone()))
        .collect()
}

pub fn help(workflows: &[Workflow]) -> String {
    let mut lines: Vec<String> = COMMANDS.iter().map(|(name, description)| format!("{name:<12} {description}")).collect();
    if !workflows.is_empty() {
        lines.push("Workflows of this project (.agents/workflows; text after the name is passed on):".into());
        lines.extend(workflows.iter().map(|(name, description)| format!("{name:<12} {description}")));
    }
    lines.join("\n")
}

pub fn suggestions(prefix: &str, workflows: &[Workflow]) -> Vec<Suggestion> {
    COMMANDS
        .iter()
        .map(|(name, description)| ((*name).to_string(), (*description).to_string()))
        .chain(workflows.iter().map(|(name, description)| (name.clone(), format!("Workflow: {description}"))))
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(value, description)| Suggestion { value, description })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_agents::workspace::{Homes, Workspace};

    #[test]
    fn test_workflows_become_commands_unless_a_built_in_has_the_name() {
        let base = std::env::temp_dir().join(format!("cluaiz-commands-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let folder = base.join(".agents/workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("release.md"), "---\ndescription: Cut a release\n---\n1. bump\n2. tag").unwrap();
        // A built-in name: the built-in command wins, the workflow is not offered
        std::fs::write(folder.join("plan.md"), "---\ndescription: Not used\n---\nx").unwrap();
        let workspace = Workspace::scan(&base, Homes::default(), &[]);

        let list = workflows(&workspace);
        assert_eq!(list, [("/release".to_string(), "Cut a release".to_string())]);
        assert!(is_built_in("plan") && is_built_in("/exit") && !is_built_in("release"));
        let offered = suggestions("/re", &list);
        assert!(offered.iter().any(|s| s.value == "/release" && s.description == "Workflow: Cut a release"));
        assert!(offered.iter().any(|s| s.value == "/rewind"));
        assert!(help(&list).contains("/release") && help(&list).contains("Workflows of this project"));
        assert!(!help(&[]).contains("Workflows of this project"));
        let _ = std::fs::remove_dir_all(base);
    }
}
