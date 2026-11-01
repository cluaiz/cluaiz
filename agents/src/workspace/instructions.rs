//! The instruction files, always read: `<CLUAIZ_HOME>/AGENTS.md`, then per folder from the
//! repository root down to the working folder `.agents/AGENTS.md` (or `.agent/AGENTS.md`) and the
//! folder's own `AGENTS.md` (or `CLAUDE.md` / `GEMINI.md`). They share one size budget; the
//! nearest files take it first.

use std::path::{Path, PathBuf};

use super::{project_folders, same_file, split_front_matter, AGENT_FOLDERS};

/// A folder's own instruction file, in this order (the first one found counts)
const NAMES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", "GEMINI.md"];
/// Most characters of all instructions together
pub const BUDGET: usize = 8_000;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Instruction {
    /// As shown to the model and the user: "~/.cluaiz/AGENTS.md", ".agents/AGENTS.md", "web/AGENTS.md"
    pub label: String,
    pub path: PathBuf,
    /// The part that fits the budget (front matter left out)
    pub text: String,
    /// Characters in the file
    pub total: usize,
}

impl Instruction {
    pub fn cut(&self) -> bool {
        self.text.chars().count() < self.total
    }
}

/// The instruction files for work in `dir`, farthest first
pub fn load(dir: &Path, cluaiz_home: Option<&Path>, budget: usize, problems: &mut Vec<String>) -> Vec<Instruction> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let mut add = |label: String, path: PathBuf| {
        if path.is_file() && !found.iter().any(|(_, seen)| same_file(seen, &path)) {
            found.push((label, path));
        }
    };
    if let Some(home) = cluaiz_home {
        add("~/.cluaiz/AGENTS.md".into(), home.join("AGENTS.md"));
    }
    let (root, folders) = project_folders(dir);
    for folder in folders {
        let relative = folder.strip_prefix(root).unwrap_or(folder);
        let label = |name: &str| relative.join(name).display().to_string().replace('\\', "/");
        // The agent folder's AGENTS.md: the published name, else the older one
        if let Some(agents) = AGENT_FOLDERS.iter().find(|name| folder.join(name).join("AGENTS.md").is_file()) {
            add(label(&format!("{agents}/AGENTS.md")), folder.join(agents).join("AGENTS.md"));
        }
        if let Some(name) = NAMES.iter().find(|name| folder.join(name).is_file()) {
            add(label(name), folder.join(name));
        }
    }

    let mut left = budget;
    let mut loaded = Vec::new();
    for (label, path) in found.into_iter().rev() {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => split_front_matter(&text).1,
            Err(error) => {
                problems.push(format!("{} is not used: {error}", path.display()));
                continue;
            }
        };
        if text.is_empty() {
            continue;
        }
        let total = text.chars().count();
        let kept: String = text.chars().take(left).collect();
        left -= kept.chars().count();
        loaded.push(Instruction { label, path, text: kept, total });
    }
    loaded.reverse();
    loaded
}
