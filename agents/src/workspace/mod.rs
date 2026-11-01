//! A project's agent files, found the way the published tools lay them out, and one index of them.
//!
//! - **Instructions** (always read): `<CLUAIZ_HOME>/AGENTS.md` for every project, then for each
//!   folder from the repository root down to the working folder its `.agents/AGENTS.md` (or the
//!   older `.agent/AGENTS.md`) and its own `AGENTS.md` (or `CLAUDE.md` / `GEMINI.md`, for a project
//!   set up for those tools). A nearer file is more specific.
//! - **Rules** `rules/*.md`, **workflows** `workflows/*.md` and **skills** `skills/<name>/SKILL.md`
//!   in each `.agents/` (and `.agent/`) folder from the working folder up to the repository root,
//!   then the user's own in `~/.agents/` (skills also in `<CLUAIZ_HOME>/tools/skills/`). A nearer
//!   item hides a farther one of the same name.
//!
//! Every item has a name and a description, from its front matter (`name`, `description`), else
//! from its file name and first heading. A rule's `trigger` is `always_on` (the default),
//! `model_decision`, `glob` (with `globs`) or `manual`, as in Antigravity. The index is built from
//! the files each time; nobody writes or maintains it.

mod instructions;

pub use instructions::{Instruction, BUDGET};

use std::path::{Path, PathBuf};

/// The folders of agent files in a project folder, the published name first
const AGENT_FOLDERS: [&str; 2] = [".agents", ".agent"];
/// Most characters of a description kept in the index
const DESCRIPTION_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Rule,
    Workflow,
    Skill,
}

impl Kind {
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Rule => "rules",
            Kind::Workflow => "workflows",
            Kind::Skill => "skills",
        }
    }
}

/// When a rule applies (Antigravity's activation modes)
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Its text is always in the prompt
    AlwaysOn,
    /// Only its name and description are shown; the model reads it when the task fits
    ModelDecision,
    /// It applies when a file matching `globs` is worked on
    Glob,
    /// Only when the user names it
    Manual,
}

impl Trigger {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "always_on" | "always" => Some(Trigger::AlwaysOn),
            "model_decision" => Some(Trigger::ModelDecision),
            "glob" => Some(Trigger::Glob),
            "manual" => Some(Trigger::Manual),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub description: String,
    /// The file to read: the rule or workflow file, or the skill's SKILL.md
    pub path: PathBuf,
    /// Where it was found, as shown to the user: `.agents/rules`, `web/.agents/skills`, `~/.agents/workflows`
    pub source: String,
    /// Rules only
    pub trigger: Option<Trigger>,
    /// Rules with trigger glob: the file patterns
    pub globs: Vec<String>,
}

impl Item {
    /// The item's text, without its front matter
    pub fn body(&self) -> Result<String, String> {
        let text = std::fs::read_to_string(&self.path).map_err(|error| format!("{}: {error}", self.path.display()))?;
        Ok(split_front_matter(&text).1)
    }

    /// A skill's folder (its SKILL.md and the files it refers to); for others the file's folder
    pub fn folder(&self) -> &Path {
        self.path.parent().unwrap_or(&self.path)
    }
}

/// Everything found for work in one folder
#[derive(Default)]
pub struct Workspace {
    /// The repository root (the folder with `.git`), else the working folder
    pub root: PathBuf,
    pub instructions: Vec<Instruction>,
    pub rules: Vec<Item>,
    pub workflows: Vec<Item>,
    pub skills: Vec<Item>,
    /// Files that could not be used as they are, and why
    pub problems: Vec<String>,
}

/// Where to look besides the project
#[derive(Clone, Copy, Default)]
pub struct Homes<'a> {
    /// `<CLUAIZ_HOME>`: its AGENTS.md and `tools/skills/`
    pub cluaiz: Option<&'a Path>,
    /// The user's home: `~/.agents/`
    pub user: Option<&'a Path>,
}

impl Workspace {
    /// Reads the agent files for work in `dir`. `installed` are the ids of skills installed from
    /// the registry (already offered with the installed tools, so not listed again).
    pub fn scan(dir: &Path, homes: Homes, installed: &[String]) -> Self {
        let (root, folders) = project_folders(dir);
        let mut workspace = Workspace { root: root.to_path_buf(), ..Default::default() };
        workspace.instructions = instructions::load(dir, homes.cluaiz, BUDGET, &mut workspace.problems);

        // Nearest first: the working folder up to the root, then the user's own
        let mut places: Vec<(PathBuf, String)> = Vec::new();
        for folder in folders.iter().rev() {
            for name in AGENT_FOLDERS {
                let relative = folder.strip_prefix(root).unwrap_or(folder).join(name);
                places.push((folder.join(name), relative.display().to_string().replace('\\', "/")));
            }
        }
        if let Some(user) = homes.user {
            places.push((user.join(".agents"), "~/.agents".into()));
        }
        for kind in [Kind::Rule, Kind::Workflow, Kind::Skill] {
            let mut found: Vec<Item> = Vec::new();
            let mut seen: Vec<PathBuf> = Vec::new();
            let mut kind_places: Vec<(PathBuf, String)> =
                places.iter().map(|(dir, label)| (dir.join(kind.folder()), format!("{label}/{}", kind.folder()))).collect();
            if kind == Kind::Skill {
                if let Some(cluaiz) = homes.cluaiz {
                    // Before ~/.agents/skills, as it was before the index existed
                    let at = kind_places.len() - usize::from(homes.user.is_some());
                    kind_places.insert(at, (cluaiz.join("tools").join("skills"), "~/.cluaiz/tools/skills".into()));
                }
            }
            for (place, source) in kind_places {
                if seen.iter().any(|done| same_file(done, &place)) {
                    continue;
                }
                seen.push(place.clone());
                let skip_installed = source == "~/.cluaiz/tools/skills";
                for item in read_items(kind, &place, &source, &mut workspace.problems) {
                    let folder = item.folder().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    if skip_installed && installed.iter().any(|id| *id == folder) {
                        continue;
                    }
                    if !found.iter().any(|other| other.name.eq_ignore_ascii_case(&item.name)) {
                        found.push(item);
                    }
                }
            }
            match kind {
                Kind::Rule => workspace.rules = found,
                Kind::Workflow => workspace.workflows = found,
                Kind::Skill => workspace.skills = found,
            }
        }
        workspace
    }

    /// Every rule, workflow and skill: the index the model and the user see
    pub fn index(&self) -> impl Iterator<Item = &Item> {
        self.rules.iter().chain(&self.workflows).chain(&self.skills)
    }

    /// An item by name (any case), of one kind or any
    pub fn find(&self, name: &str, kind: Option<Kind>) -> Option<&Item> {
        let name = name.trim().trim_start_matches('/');
        self.index().find(|item| kind.is_none_or(|kind| item.kind == kind) && item.name.eq_ignore_ascii_case(name))
    }
}

/// The items of one kind in one folder, sorted by file name
fn read_items(kind: Kind, place: &Path, source: &str, problems: &mut Vec<String>) -> Vec<Item> {
    let Ok(entries) = std::fs::read_dir(place) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| match kind {
            Kind::Skill => Some(path.join("SKILL.md")).filter(|file| file.is_file()),
            _ => Some(path).filter(|file| file.is_file() && file.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))),
        })
        .collect();
    files.sort();
    files.into_iter().filter_map(|file| read_item(kind, &file, source, problems)).collect()
}

fn read_item(kind: Kind, file: &Path, source: &str, problems: &mut Vec<String>) -> Option<Item> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            problems.push(format!("{} is not used: {error}", file.display()));
            return None;
        }
    };
    let (front, body) = split_front_matter(&text);
    let field = |key: &str| front.get(key).and_then(serde_yaml::Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    // A skill is named after its folder, a rule or workflow after its file
    let stem = match kind {
        Kind::Skill => file.parent().and_then(Path::file_name),
        _ => file.file_stem(),
    }
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_default();
    let name = field("name").unwrap_or(stem);
    let description = field("description").unwrap_or_else(|| first_line(&body));
    let description: String = description.chars().take(DESCRIPTION_CHARS).collect();

    let (mut trigger, mut globs) = (None, Vec::new());
    if kind == Kind::Rule {
        trigger = Some(match field("trigger") {
            None => Trigger::AlwaysOn,
            Some(value) => Trigger::parse(&value).unwrap_or_else(|| {
                problems.push(format!(
                    "{}: trigger \"{value}\" is not always_on, model_decision, glob or manual; it is used as always_on",
                    file.display()
                ));
                Trigger::AlwaysOn
            }),
        });
        globs = match front.get("globs").or_else(|| front.get("glob")) {
            Some(serde_yaml::Value::Sequence(list)) => list.iter().filter_map(serde_yaml::Value::as_str).map(|g| g.trim().to_owned()).collect(),
            Some(serde_yaml::Value::String(list)) => list.split(',').map(|g| g.trim().to_owned()).collect(),
            _ => Vec::new(),
        };
        globs.retain(|g| !g.is_empty());
        if trigger == Some(Trigger::Glob) && globs.is_empty() {
            problems.push(format!("{}: trigger glob has no globs; it is used as model_decision", file.display()));
            trigger = Some(Trigger::ModelDecision);
        }
        if trigger == Some(Trigger::ModelDecision) && field("description").is_none() {
            problems.push(format!("{}: a model_decision rule needs a description; its first line is shown instead", file.display()));
        }
    }
    Some(Item { kind, name, description, path: file.to_path_buf(), source: source.to_owned(), trigger, globs })
}

/// `---` YAML front matter (empty when there is none or it does not parse) and the text under it
pub fn split_front_matter(text: &str) -> (serde_yaml::Mapping, String) {
    let text = text.replace("\r\n", "\n");
    let text = text.trim_start_matches('\u{feff}');
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let after = &rest[end + 4..];
            // The closing line is `---` alone
            if after.is_empty() || after.starts_with('\n') {
                let front = serde_yaml::from_str::<serde_yaml::Mapping>(&rest[..end]).unwrap_or_default();
                return (front, after.trim().to_string());
            }
        }
    }
    (serde_yaml::Mapping::new(), text.trim().to_string())
}

/// The first heading or line of a text, without Markdown marks
fn first_line(body: &str) -> String {
    body.lines()
        .map(|line| line.trim().trim_start_matches('#').trim().trim_start_matches('>').trim())
        .find(|line| !line.is_empty() && !line.starts_with("```"))
        .unwrap_or_default()
        .to_string()
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

#[cfg(test)]
mod tests;
