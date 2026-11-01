//! The project's agent files in the agent: the AGENTS.md instructions in the system prompt, and
//! what /memory shows. Finding the files is `cluaiz_agents::workspace` (shared with the Engine):
//! `~/.cluaiz/AGENTS.md`, then per folder from the repository root down `.agents/AGENTS.md` and the
//! folder's own `AGENTS.md` (or `CLAUDE.md` / `GEMINI.md`); rules, workflows and skills in
//! `.agents/`.

use std::path::Path;

use cluaiz_agents::workspace::{Homes, Instruction, Kind, Trigger, Workspace, BUDGET};

/// The agent files for work in `dir`
pub fn scan(dir: &Path, installed: &[String]) -> Workspace {
    let cluaiz = cluaiz_core::EnvironmentManager::current().global_dir;
    let user = user_home();
    Workspace::scan(dir, Homes { cluaiz: Some(&cluaiz), user: user.as_deref() }, installed)
}

/// The user's home folder (for `~/.agents/`)
pub fn user_home() -> Option<std::path::PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from)
}

/// The instructions as a block of the system prompt (empty without any)
pub fn prompt(loaded: &[Instruction]) -> String {
    if loaded.iter().all(|file| file.text.is_empty()) {
        return String::new();
    }
    let mut block = String::from(
        "Instructions from the user and the project. Follow them; when two disagree, the later (more specific) one wins.",
    );
    for file in loaded.iter().filter(|file| !file.text.is_empty()) {
        let cut = if file.cut() { " (cut to fit)" } else { "" };
        block.push_str(&format!("\n\n## {}{cut}\n{}", file.label, file.text));
    }
    block
}

fn trigger_name(trigger: Option<Trigger>) -> &'static str {
    match trigger {
        Some(Trigger::AlwaysOn) | None => "always on",
        Some(Trigger::ModelDecision) => "when it fits",
        Some(Trigger::Glob) => "for matching files",
        Some(Trigger::Manual) => "when named",
    }
}

/// What /memory shows: the instruction files and how much of each is in the prompt, the rules and
/// workflows found, files that could not be used, and where the files go
pub fn summary(workspace: &Workspace, home: Option<&Path>) -> String {
    let display = |path: &Path| cluaiz_api::cep::roots::display(path);
    let mut places = Vec::new();
    if let Some(home) = home {
        places.push(format!("  For every project:  {}", display(&home.join("AGENTS.md"))));
    }
    let agents = workspace.root.join(".agents");
    places.push(format!("  For this project:   {}", display(&agents.join("AGENTS.md"))));
    places.push(format!("  Rules:              {}", display(&agents.join("rules"))));
    places.push(format!("  Workflows:          {}", display(&agents.join("workflows"))));
    places.push(format!("  Skills:             {}", display(&agents.join("skills"))));

    let mut lines = Vec::new();
    let loaded = &workspace.instructions;
    if loaded.is_empty() {
        lines.push("No AGENTS.md instructions yet (type /init: the model reads this project and writes one).".to_string());
    } else {
        let width = loaded.iter().map(|file| file.label.len()).max().unwrap_or(0);
        lines.push(format!("Instructions in the prompt ({BUDGET} characters at most, nearest file first):"));
        for file in loaded {
            let used = file.text.chars().count();
            let size = match (used, file.cut()) {
                (0, _) => format!("left out: no room ({} characters)", file.total),
                (_, true) => format!("{used} of {} characters (cut)", file.total),
                _ => format!("{used} characters"),
            };
            lines.push(format!("  {:<width$}  {size}", file.label));
        }
    }
    for (kind, title) in [(Kind::Rule, "Rules"), (Kind::Workflow, "Workflows")] {
        let items: Vec<_> = workspace.index().filter(|item| item.kind == kind).collect();
        if items.is_empty() {
            continue;
        }
        lines.push(format!("{title}:"));
        let width = items.iter().map(|item| item.name.len()).max().unwrap_or(0);
        for item in items {
            let when = if kind == Kind::Rule { format!("{}  ", trigger_name(item.trigger)) } else { String::new() };
            let description: String = item.description.chars().take(70).collect();
            lines.push(format!("  {:<width$}  {when}{description}  [{}]", item.name, item.source));
        }
    }
    for problem in &workspace.problems {
        lines.push(format!("! {problem}"));
    }
    lines.push("Edits take effect in a new agent session. The files:".into());
    lines.extend(places);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_keeps_the_order_and_summary_shows_rules_and_places() {
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
        let block = prompt(&workspace.instructions);
        assert!(block.find("Answer briefly").unwrap() < block.find("This app uses Vite").unwrap());
        let text = summary(&workspace, Some(&home));
        assert!(text.contains(".agents/AGENTS.md"));
        assert!(text.contains("style") && text.contains("when it fits") && text.contains("Code style"));
        assert!(text.contains(&cluaiz_api::cep::roots::display(&repo.join(".agents").join("rules"))));

        let empty = Workspace::scan(&base.join("none"), Homes::default(), &[]);
        assert!(prompt(&empty.instructions).is_empty());
        assert!(summary(&empty, None).contains("/init"));
        let _ = std::fs::remove_dir_all(base);
    }
}
