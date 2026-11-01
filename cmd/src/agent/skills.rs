//! Skills the user and the project wrote, in the Agent Skills format (a folder with a `SKILL.md`:
//! `name` and `description` in its front matter, the instructions under it), which Codex, Cursor,
//! Copilot and Antigravity read as well. `cluaiz_agents::workspace` finds them, nearest first:
//! `.agents/skills/` (or `.agent/skills/`) from the working folder up to the repository root,
//! `~/.cluaiz/tools/skills/` (a skill written there by hand), `~/.agents/skills/`.
//!
//! Only each skill's name and description go into the prompt (the workspace index), so a small
//! context stays free; the model calls `read_item` to read the one that fits the task. This file
//! is what /skills shows.

use std::path::Path;

use cluaiz_agents::workspace::Item;

/// What /skills shows
pub fn summary(skills: &[Item], dir: &Path, user_home: Option<&Path>) -> String {
    if skills.is_empty() {
        let mut places = vec![format!("  For this project:   {}", cluaiz_api::cep::roots::display(&dir.join(".agents").join("skills")))];
        if let Some(home) = user_home {
            places.push(format!("  For every project:  {}", cluaiz_api::cep::roots::display(&home.join(".agents").join("skills"))));
        }
        return format!(
            "No skills yet. A skill is a folder with a SKILL.md (name and description in its front matter, the \
             instructions under it), for example <folder>/my-skill/SKILL.md, in:\n{}",
            places.join("\n")
        );
    }
    let width = skills.iter().map(|skill| skill.name.len()).max().unwrap_or(0);
    let mut lines = vec!["Skills (the model reads one with read_item when the task fits):".to_string()];
    for skill in skills {
        let description: String = skill.description.chars().take(80).collect();
        lines.push(format!("  {:<width$}  {}  [{}]", skill.name, description, skill.source));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_agents::workspace::{Homes, Workspace};

    #[test]
    fn test_skills_are_listed_with_their_source_and_none_says_where_they_go() {
        let base = std::env::temp_dir().join(format!("cluaiz-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let folder = base.join(".agents/skills/react");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("SKILL.md"), "---\nname: react-app\ndescription: Create or change the React app\n---\nUse npm create vite.\n").unwrap();
        let skills = Workspace::scan(&base, Homes::default(), &[]).skills;
        let text = summary(&skills, &base, None);
        assert!(text.contains("react-app") && text.contains("Create or change the React app") && text.contains("[.agents/skills]"));
        let none = summary(&[], &base, None);
        assert!(none.contains(&cluaiz_api::cep::roots::display(&base.join(".agents").join("skills"))));
        let _ = std::fs::remove_dir_all(base);
    }
}
