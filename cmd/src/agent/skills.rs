//! Skills the user and the project wrote, in the Agent Skills format (a folder with a `SKILL.md`:
//! `name` and `description` in its front matter, the instructions under it), which Codex, Cursor,
//! Copilot and Antigravity read as well. `cluaiz_agents::workspace` finds them, nearest first:
//! `.agents/skills/` (or `.agent/skills/`) from the working folder up to the repository root,
//! `~/.cluaiz/tools/skills/` (a skill written there by hand), `~/.agents/skills/`.
//!
//! Only each skill's name and description go into the prompt, so a small context stays free; the
//! model calls `use_skill` to read the instructions of the one that fits the task.

use std::path::Path;

use cluaiz_agents::workspace::Item;

/// Most characters of one description in the prompt
const DESCRIPTION_CHARS: usize = 200;
/// Most characters of the whole skill list in the prompt
const LIST_CHARS: usize = 3_000;

/// The skill list for the system prompt (empty without skills)
pub fn prompt(skills: &[Item]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut block = String::from(
        "Skills (instructions for particular kinds of task). When the task matches one, call use_skill with its name before you start, then follow what it says:",
    );
    let mut left_out = 0;
    for skill in skills {
        let description: String = skill.description.chars().take(DESCRIPTION_CHARS).collect();
        let line = format!("\n- {}: {}", skill.name, if description.is_empty() { "(no description)" } else { &description });
        if block.len() + line.len() > LIST_CHARS {
            left_out += 1;
            continue;
        }
        block.push_str(&line);
    }
    if left_out > 0 {
        block.push_str(&format!("\n({left_out} more skills; use_skill with any name lists them all)"));
    }
    block
}

/// What `use_skill` returns: the skill's instructions and its folder
pub fn read(skills: &[Item], name: &str) -> Result<(String, String), String> {
    let wanted = name.trim();
    let Some(skill) = skills.iter().find(|skill| skill.name.eq_ignore_ascii_case(wanted)) else {
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        return Err(format!("there is no skill named '{wanted}'. Skills: {}", names.join(", ")));
    };
    let body = skill.body()?;
    let output = format!("Skill {} (its files are in {}):\n\n{}", skill.name, skill.folder().display(), body);
    Ok((format!("{} · {} lines", skill.name, body.lines().count()), output))
}

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
    let mut lines = vec!["Skills (the model reads one with use_skill when the task fits):".to_string()];
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
    fn test_only_descriptions_go_into_the_prompt_and_use_skill_reads_the_body() {
        let base = std::env::temp_dir().join(format!("cluaiz-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let folder = base.join(".agents/skills/react");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("SKILL.md"), "---\nname: react-app\ndescription: Create or change the React app\n---\nUse npm create vite.\n").unwrap();
        let skills = Workspace::scan(&base, Homes::default(), &[]).skills;

        let block = prompt(&skills);
        assert!(block.contains("- react-app: Create or change the React app"));
        assert!(!block.contains("Use npm create vite"), "only descriptions go into the prompt");
        let (summary_line, text) = read(&skills, "React-App").unwrap();
        assert!(summary_line.starts_with("react-app"));
        assert!(text.contains("Use npm create vite."));
        assert!(read(&skills, "nope").unwrap_err().contains("react-app"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn test_a_long_list_is_cut_and_says_so() {
        let skills: Vec<Item> = (0..100)
            .map(|i| Item {
                kind: cluaiz_agents::workspace::Kind::Skill,
                name: format!("skill-{i}"),
                description: "d".repeat(150),
                path: std::path::PathBuf::new(),
                source: String::new(),
                trigger: None,
                globs: Vec::new(),
            })
            .collect();
        let block = prompt(&skills);
        assert!(block.len() <= LIST_CHARS + 100);
        assert!(block.contains("more skills; use_skill with any name lists them all"));
    }
}
