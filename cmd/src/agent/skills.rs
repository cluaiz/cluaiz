//! Skills the user and the project wrote, in the Agent Skills format (a folder with a `SKILL.md`:
//! `name` and `description` in its front matter, the instructions under it), which Codex, Cursor,
//! Copilot and Antigravity read as well. They are found in, nearest first:
//!
//! 1. `.agents/skills/` of the working folder and of each folder up to the repository root
//! 2. `~/.cluaiz/tools/skills/` (a skill written there by hand; installed ones are already offered)
//! 3. `~/.agents/skills/` (the user's skills for every agent)
//!
//! Only each skill's name and description go into the prompt, so a small context stays free; the
//! model calls `use_skill` to read the instructions of the one that fits the task.

use std::path::{Path, PathBuf};

/// Most characters of one description in the prompt
const DESCRIPTION_CHARS: usize = 200;
/// Most characters of the whole skill list in the prompt
const LIST_CHARS: usize = 3_000;

pub struct Skill {
    pub name: String,
    pub description: String,
    /// The skill's folder (its SKILL.md and any files it refers to)
    pub dir: PathBuf,
    /// Where it was found: ".agents/skills", "~/.cluaiz/tools/skills", "~/.agents/skills"
    pub source: String,
}

/// The skills for work in `dir`; a nearer skill hides a farther one of the same name. `installed`
/// are the registry ids already offered with their instructions.
pub fn discover(dir: &Path, cluaiz_home: Option<&Path>, user_home: Option<&Path>, installed: &[String]) -> Vec<Skill> {
    let mut places: Vec<(PathBuf, String)> = Vec::new();
    let (root, folders) = super::instructions::project_folders(dir);
    for folder in folders.into_iter().rev() {
        let relative = folder.strip_prefix(root).unwrap_or(folder).join(".agents").join("skills");
        places.push((folder.join(".agents").join("skills"), relative.display().to_string().replace('\\', "/")));
    }
    if let Some(home) = cluaiz_home {
        places.push((home.join("tools").join("skills"), "~/.cluaiz/tools/skills".into()));
    }
    if let Some(home) = user_home {
        places.push((home.join(".agents").join("skills"), "~/.agents/skills".into()));
    }

    let mut skills: Vec<Skill> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for (place, source) in places {
        if seen.iter().any(|done| super::instructions::same_file(done, &place)) {
            continue;
        }
        seen.push(place.clone());
        let Ok(entries) = std::fs::read_dir(&place) else { continue };
        let mut folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.join("SKILL.md").is_file()).collect();
        folders.sort();
        for folder in folders {
            let folder_name = folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if source == "~/.cluaiz/tools/skills" && installed.iter().any(|id| *id == folder_name) {
                continue;
            }
            let parsed = cluaiz_agents::SkillParser::parse_file(folder.join("SKILL.md"));
            let (name, description) = match &parsed {
                Some(skill) => (skill.metadata.name.trim().to_string(), skill.metadata.description.trim().to_string()),
                None => (String::new(), String::new()),
            };
            let name = if name.is_empty() { folder_name } else { name };
            if skills.iter().any(|skill| skill.name.eq_ignore_ascii_case(&name)) {
                continue;
            }
            skills.push(Skill { name, description, dir: folder, source: source.clone() });
        }
    }
    skills
}

/// The skill list for the system prompt (empty without skills)
pub fn prompt(skills: &[Skill]) -> String {
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
pub fn read(skills: &[Skill], name: &str) -> Result<(String, String), String> {
    let wanted = name.trim();
    let Some(skill) = skills.iter().find(|skill| skill.name.eq_ignore_ascii_case(wanted)) else {
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        return Err(format!("there is no skill named '{wanted}'. Skills: {}", names.join(", ")));
    };
    let file = skill.dir.join("SKILL.md");
    let text = std::fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?;
    let body = cluaiz_agents::SkillParser::parse_content(&text)
        .map(|parsed| parsed.prompt_instructions)
        .unwrap_or_else(|| text.trim().to_string());
    let output = format!(
        "Skill {} (its files are in {}):\n\n{}",
        skill.name,
        skill.dir.display(),
        body
    );
    Ok((format!("{} · {} lines", skill.name, body.lines().count()), output))
}

/// What /skills shows
pub fn summary(skills: &[Skill], dir: &Path, user_home: Option<&Path>) -> String {
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

    fn skill(dir: &Path, folder: &str, name: &str, description: &str, body: &str) {
        let path = dir.join(folder);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("SKILL.md"), format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n")).unwrap();
    }

    #[test]
    fn test_skills_are_found_nearest_first_and_only_listed_by_description() {
        let base = std::env::temp_dir().join(format!("cluaiz-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let web = repo.join("web");
        let user = base.join("user");
        let cluaiz = base.join("cluaiz");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&web).unwrap();
        skill(&repo.join(".agents/skills"), "release", "release", "Cut a release", "Run the release workflow.");
        skill(&web.join(".agents/skills"), "react", "react-app", "Create or change the React app", "Use npm create vite.");
        // The same name farther away is hidden by the nearer one
        skill(&user.join(".agents/skills"), "react2", "react-app", "older", "old");
        skill(&user.join(".agents/skills"), "review", "review", "Review a change", "Look for bugs.");
        // An installed skill is already offered; a hand-written one in the same folder is not
        skill(&cluaiz.join("tools/skills"), "installed", "installed", "from the registry", "x");
        skill(&cluaiz.join("tools/skills"), "notes", "notes", "Write meeting notes", "Use headings.");

        let skills = discover(&web, Some(&cluaiz), Some(&user), &["installed".to_string()]);
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["react-app", "release", "notes", "review"]);
        assert_eq!(skills[0].source, "web/.agents/skills");

        let block = prompt(&skills);
        assert!(block.contains("- react-app: Create or change the React app"));
        assert!(!block.contains("Use npm create vite"), "only descriptions go into the prompt");

        let (summary, text) = read(&skills, "React-App").unwrap();
        assert!(summary.starts_with("react-app"));
        assert!(text.contains("Use npm create vite."));
        assert!(read(&skills, "nope").unwrap_err().contains("react-app, release, notes, review"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn test_a_long_list_is_cut_and_says_so() {
        let skills: Vec<Skill> = (0..100)
            .map(|i| Skill { name: format!("skill-{i}"), description: "d".repeat(150), dir: PathBuf::new(), source: String::new() })
            .collect();
        let block = prompt(&skills);
        assert!(block.len() <= LIST_CHARS + 100);
        assert!(block.contains("more skills; use_skill with any name lists them all"));
    }
}
