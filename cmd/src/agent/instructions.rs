//! The instructions the agent follows, from AGENTS.md files (the convention Codex, Cursor, Copilot,
//! Gemini CLI and Antigravity read too): `~/.cluaiz/AGENTS.md` for every project, then one file per
//! folder from the repository root down to the working folder, so a nearer file adds to and
//! overrides a farther one. A folder without AGENTS.md may have CLAUDE.md or GEMINI.md instead
//! (a project set up for those tools). All of them share one size budget; the nearest come first.

use std::path::{Path, PathBuf};

/// The file a folder's instructions are in, in this order (the first one found counts)
const NAMES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", "GEMINI.md"];
/// Most characters of all instructions together in the system prompt
pub const BUDGET: usize = 8_000;

pub struct Instruction {
    /// For the prompt and /memory: "~/.cluaiz/AGENTS.md", "AGENTS.md", "src/AGENTS.md"
    pub label: String,
    /// The part that is in the prompt
    pub text: String,
    /// Characters in the file
    pub total: usize,
}

impl Instruction {
    pub fn cut(&self) -> bool {
        self.text.chars().count() < self.total
    }
}

/// The instruction files for work in `dir`, farthest first; `home` is the Cluaiz home
pub fn load(dir: &Path, home: Option<&Path>) -> Vec<Instruction> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    if let Some(home) = home {
        found.push(("~/.cluaiz/AGENTS.md".into(), home.join("AGENTS.md")));
    }
    let (root, folders) = project_folders(dir);
    for folder in folders {
        let Some(name) = NAMES.iter().find(|name| folder.join(name).is_file()) else { continue };
        let relative = folder.strip_prefix(root).unwrap_or(folder);
        let label = relative.join(name).display().to_string().replace('\\', "/");
        let path = folder.join(name);
        if !found.iter().any(|(_, seen)| same_file(seen, &path)) {
            found.push((label, path));
        }
    }

    // The nearest files take the budget first
    let mut left = BUDGET;
    let mut loaded = Vec::new();
    for (label, path) in found.into_iter().rev() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let total = text.chars().count();
        let kept: String = text.chars().take(left).collect();
        left -= kept.chars().count();
        loaded.push(Instruction { label, text: kept, total });
    }
    loaded.reverse();
    loaded
}

/// The repository root (the folder with `.git`) and the folders from it down to `dir`, root first;
/// outside a repository only `dir`, so files of unrelated parent folders (the home folder) are
/// never read
pub fn project_folders(dir: &Path) -> (&Path, Vec<&Path>) {
    let root = dir.ancestors().find(|folder| folder.join(".git").exists()).unwrap_or(dir);
    let mut folders: Vec<&Path> = dir.ancestors().take_while(|folder| *folder != root).collect();
    folders.push(root);
    folders.reverse();
    (root, folders)
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
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

/// What /memory shows: each file and how much of it is in the prompt
pub fn summary(loaded: &[Instruction]) -> String {
    if loaded.is_empty() {
        return "No instructions loaded. Write rules for this project in AGENTS.md, or for every project in \
                ~/.cluaiz/AGENTS.md; they are read when the agent starts."
            .into();
    }
    let width = loaded.iter().map(|file| file.label.len()).max().unwrap_or(0);
    let mut lines = vec![format!("Instructions in the prompt ({BUDGET} characters at most, nearest file first):")];
    for file in loaded {
        let used = file.text.chars().count();
        let size = match (used, file.cut()) {
            (0, _) => format!("left out: no room ({} characters)", file.total),
            (_, true) => format!("{used} of {} characters (cut)", file.total),
            _ => format!("{used} characters"),
        };
        lines.push(format!("  {:<width$}  {size}", file.label));
    }
    lines.push("Edits take effect in a new agent session.".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("cluaiz-instructions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn test_files_from_home_and_repository_root_down_to_the_folder() {
        let base = tree("order");
        let home = base.join("home");
        let repo = base.join("repo");
        let app = repo.join("apps").join("web");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(home.join("AGENTS.md"), "Answer briefly.").unwrap();
        std::fs::write(repo.join("AGENTS.md"), "Use npm.").unwrap();
        // A folder set up for another tool: its file counts when AGENTS.md is not there
        std::fs::write(repo.join("apps").join("CLAUDE.md"), "Apps use React.").unwrap();
        std::fs::write(app.join("AGENTS.md"), "This app uses Vite.").unwrap();
        std::fs::write(app.join("GEMINI.md"), "ignored: AGENTS.md comes first").unwrap();

        let loaded = load(&app, Some(&home));
        let labels: Vec<_> = loaded.iter().map(|file| file.label.as_str()).collect();
        assert_eq!(labels, ["~/.cluaiz/AGENTS.md", "AGENTS.md", "apps/CLAUDE.md", "apps/web/AGENTS.md"]);
        let block = prompt(&loaded);
        assert!(block.find("Answer briefly").unwrap() < block.find("This app uses Vite").unwrap());
        assert!(!block.contains("ignored"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn test_outside_a_repository_only_the_folder_counts() {
        let base = tree("norepo");
        let dir = base.join("work");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(base.join("AGENTS.md"), "a parent folder's file").unwrap();
        std::fs::write(dir.join("AGENTS.md"), "Work notes.").unwrap();
        let loaded = load(&dir, None);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "Work notes.");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn test_the_nearest_file_keeps_its_text_when_the_budget_runs_out() {
        let base = tree("budget");
        let home = base.join("home");
        let dir = base.join("work");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(home.join("AGENTS.md"), "h".repeat(3_000)).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "p".repeat(BUDGET - 1_000)).unwrap();
        let loaded = load(&dir, Some(&home));
        assert_eq!(loaded[1].text.len(), BUDGET - 1_000);
        assert!(!loaded[1].cut());
        assert_eq!(loaded[0].text.len(), 1_000);
        assert!(loaded[0].cut());
        assert!(summary(&loaded).contains("1000 of 3000 characters (cut)"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn test_no_files_tells_where_to_write_them() {
        let base = tree("none");
        assert!(load(&base, None).is_empty());
        assert!(prompt(&[]).is_empty());
        assert!(summary(&[]).contains("~/.cluaiz/AGENTS.md"));
        let _ = std::fs::remove_dir_all(base);
    }
}
