//! The project's agent files in the agent. Finding them and putting them into the system prompt is
//! `cluaiz_agents::workspace` (shared with the Engine): `~/.cluaiz/AGENTS.md`, then per folder from
//! the repository root down `.agents/AGENTS.md` and the folder's own `AGENTS.md` (or `CLAUDE.md` /
//! `GEMINI.md`); rules, workflows and skills in `.agents/`. This file scans them for the agent and
//! is what /memory shows.

use std::path::Path;

use cluaiz_agents::workspace::{Homes, Kind, Placement, Trigger, Workspace, WorkspacePrompt};

/// The agent files for work in `dir`; `installed` are the registry ids of installed skills
pub fn scan(dir: &Path, installed: &[String]) -> Workspace {
    let cluaiz = cluaiz_core::EnvironmentManager::current().global_dir;
    let user = user_home();
    Workspace::scan(dir, Homes { cluaiz: Some(&cluaiz), user: user.as_deref() }, installed)
}

/// The user's home folder (for `~/.agents/`)
pub fn user_home() -> Option<std::path::PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from)
}

fn trigger_name(trigger: Option<Trigger>) -> &'static str {
    match trigger {
        Some(Trigger::AlwaysOn) | None => "always on",
        Some(Trigger::ModelDecision) => "when it fits",
        Some(Trigger::Glob) => "for matching files",
        Some(Trigger::Manual) => "when named",
    }
}

fn placement_name(placement: Placement) -> &'static str {
    match placement {
        Placement::Whole => "in the prompt",
        Placement::Cut => "in the prompt, cut",
        Placement::Index => "in the index (read on demand)",
        Placement::Left => "not in the prompt",
    }
}

/// What /memory shows: what of the project's agent files is in the system prompt and how much
/// room it takes, the rules and workflows found, files that could not be used, and where the files go
pub fn summary(workspace: &Workspace, layout: &WorkspacePrompt, home: Option<&Path>) -> String {
    let display = |path: &Path| cluaiz_api::cep::roots::display(path);
    let mut lines = Vec::new();
    if layout.parts.is_empty() {
        lines.push("No AGENTS.md, rules, workflows or skills yet (type /init: the model reads this project and writes .agents/AGENTS.md).".to_string());
    } else {
        lines.push(format!(
            "In the system prompt: {} of {} characters (about {} tokens; the share follows the model's context)",
            layout.text.chars().count(),
            layout.budget,
            layout.text.chars().count() / 4
        ));
        let width = layout.parts.iter().map(|part| part.name.len()).max().unwrap_or(0);
        for part in &layout.parts {
            let size = match part.placement {
                Placement::Cut => format!("  {} of {} characters", part.chars, part.total),
                Placement::Whole => format!("  {} characters", part.chars),
                _ => String::new(),
            };
            lines.push(format!("  {:<width$}  {:<12}  {}{size}", part.name, part.kind, placement_name(part.placement)));
        }
    }
    let rules: Vec<_> = workspace.index().filter(|item| item.kind == Kind::Rule).collect();
    if !rules.is_empty() {
        lines.push("Rules:".into());
        let width = rules.iter().map(|item| item.name.len()).max().unwrap_or(0);
        for item in rules {
            let description: String = item.description.chars().take(70).collect();
            lines.push(format!("  {:<width$}  {:<18}  {description}  [{}]", item.name, trigger_name(item.trigger), item.source));
        }
    }
    for problem in &workspace.problems {
        lines.push(format!("! {problem}"));
    }
    let agents = workspace.root.join(".agents");
    lines.push("Edits take effect in a new agent session. The files:".into());
    if let Some(home) = home {
        lines.push(format!("  For every project:  {}", display(&home.join("AGENTS.md"))));
    }
    lines.push(format!("  For this project:   {}", display(&agents.join("AGENTS.md"))));
    lines.push(format!("  Rules:              {}", display(&agents.join("rules"))));
    lines.push(format!("  Workflows:          {}", display(&agents.join("workflows"))));
    lines.push(format!("  Skills:             {}", display(&agents.join("skills"))));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_shows_what_went_into_the_prompt_and_where_files_go() {
        let base = std::env::temp_dir().join(format!("cluaiz-instructions-summary-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (home, repo) = (base.join("home"), base.join("repo"));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join(".agents/rules")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("AGENTS.md"), "Answer briefly.").unwrap();
        std::fs::write(repo.join(".agents/AGENTS.md"), "This app uses Vite.").unwrap();
        std::fs::write(repo.join(".agents/rules/style.md"), "---\ntrigger: model_decision\ndescription: Code style\n---\nTabs.").unwrap();

        let workspace = Workspace::scan(&repo, Homes { cluaiz: Some(&home), user: None }, &[]);
        let layout = workspace.prompt(8_192);
        assert!(layout.text.find("Answer briefly").unwrap() < layout.text.find("This app uses Vite").unwrap());
        let text = summary(&workspace, &layout, Some(&home));
        assert!(text.contains(".agents/AGENTS.md") && text.contains("in the prompt"));
        assert!(text.contains("style") && text.contains("in the index") && text.contains("when it fits"));
        assert!(text.contains(&cluaiz_api::cep::roots::display(&repo.join(".agents").join("rules"))));

        let empty = Workspace::scan(&base.join("none"), Homes::default(), &[]);
        assert!(summary(&empty, &empty.prompt(8_192), None).contains("/init"));
        let _ = std::fs::remove_dir_all(base);
    }
}
