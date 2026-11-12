//! Flow files and what the owner decided about them (docs/automation/file.md#where-flows-are-kept).
//! A flow is named by `name` and `scope`; without a scope the project's flow is found first.

use crate::format::{parse_text, Format};
use crate::journal::Paths;
use crate::problem::Problem;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Project,
    User,
}

impl Scope {
    pub fn parse(text: &str) -> Option<Scope> {
        match text {
            "project" => Some(Scope::Project),
            "user" => Some(Scope::User),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::User => "user",
        }
    }
}

/// A flow file as it is on disk
#[derive(Debug, Clone)]
pub struct FlowFile {
    pub name: String,
    pub scope: Scope,
    pub path: PathBuf,
    pub format: Format,
    pub text: String,
    /// The parsed flow, or why it does not parse
    pub flow: Result<Value, Problem>,
    pub modified: Option<std::time::SystemTime>,
}

pub struct Store {
    /// `<Cluaiz folder>/flows`
    pub user: PathBuf,
    /// `<project>/.agents/flows`, when there is a current project
    pub project: Option<PathBuf>,
    pub paths: Paths,
}

impl Store {
    pub fn new(paths: Paths, project_root: Option<&Path>) -> Self {
        Self { user: paths.flows.clone(), project: project_root.map(|root| root.join(".agents").join("flows")), paths }
    }

    pub fn dir(&self, scope: Scope) -> Option<&Path> {
        match scope {
            Scope::User => Some(&self.user),
            Scope::Project => self.project.as_deref(),
        }
    }

    fn read_dir(&self, scope: Scope) -> Vec<FlowFile> {
        let Some(dir) = self.dir(scope) else { return Vec::new() };
        let mut out: Vec<FlowFile> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let (format, name) = Format::of_file(&file_name)?;
                let text = std::fs::read_to_string(entry.path()).ok()?;
                Some(FlowFile {
                    name: name.to_string(),
                    scope,
                    path: entry.path(),
                    format,
                    flow: parse_text(&text, format),
                    text,
                    modified: entry.metadata().and_then(|m| m.modified()).ok(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// The flows of one scope, or of both (the project's first)
    pub fn list(&self, scope: Option<Scope>) -> Vec<FlowFile> {
        match scope {
            Some(scope) => self.read_dir(scope),
            None => [Scope::Project, Scope::User].into_iter().flat_map(|s| self.read_dir(s)).collect(),
        }
    }

    pub fn find(&self, name: &str, scope: Option<Scope>) -> Option<FlowFile> {
        self.list(scope).into_iter().find(|f| f.name == name)
    }

    /// Writes the text as `<name>.flow.<format>`; another format of the same name is removed
    pub fn save(&self, scope: Scope, name: &str, format: Format, text: &str) -> std::io::Result<PathBuf> {
        let dir = self.dir(scope).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "there is no current project"))?;
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{name}{}", format.suffix()));
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, &path)?;
        for other in [".flow.json", ".flow.yaml", ".flow.yml"] {
            let old = dir.join(format!("{name}{other}"));
            if old != path && old.exists() {
                std::fs::remove_file(old)?;
            }
        }
        Ok(path)
    }

    pub fn remove(&self, file: &FlowFile) -> std::io::Result<()> {
        std::fs::remove_file(&file.path)
    }

    fn settings_path(&self, scope: Scope, name: &str) -> PathBuf {
        self.paths.state_dir(scope.as_str(), name).join("flow.json")
    }

    /// What the owner decided: `{ enabled, permissions, hash }`
    pub fn settings(&self, scope: Scope, name: &str) -> Value {
        crate::journal::read_json(&self.settings_path(scope, name)).unwrap_or_else(|| json!({ "enabled": false, "permissions": null }))
    }

    pub fn set_settings(&self, scope: Scope, name: &str, settings: &Value) -> std::io::Result<()> {
        let path = self.settings_path(scope, name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(settings).unwrap_or_default())
    }

    pub fn forget(&self, scope: Scope, name: &str) {
        let _ = std::fs::remove_dir_all(self.paths.state_dir(scope.as_str(), name));
    }
}

/// The flow text with `permissions` written into it. JSON is written again from the value. YAML
/// keeps the text as sent, comments too: its top-level `permissions:` block is replaced, or one is
/// added at the end. Only when that does not read back as intended is the YAML written again.
pub fn with_permissions(text: &str, format: Format, flow: &Value, permissions: &Value) -> String {
    let mut updated = flow.clone();
    if let Some(map) = updated.as_object_mut() {
        map.insert("permissions".into(), permissions.clone());
    }
    if format == Format::Json {
        return serde_json::to_string_pretty(&updated).unwrap_or_else(|_| text.to_string()) + "\n";
    }
    let block = serde_yaml::to_string(&json!({ "permissions": permissions })).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let edited = match lines.iter().position(|l| l.starts_with("permissions:")) {
        // The block: its first line and the indented lines under it
        Some(start) => {
            let end = (start + 1..lines.len()).find(|&i| !lines[i].is_empty() && !lines[i].starts_with([' ', '\t'])).unwrap_or(lines.len());
            let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
            out.extend(block.lines().map(String::from));
            out.extend(lines[end..].iter().map(|l| l.to_string()));
            out.join("\n") + "\n"
        }
        None => format!("{}\n{block}", text.trim_end()),
    };
    match parse_text(&edited, Format::Yaml) {
        Ok(value) if value == updated => edited,
        _ => serde_yaml::to_string(&updated).unwrap_or_else(|_| text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_first_and_one_format_per_name() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths { flows: dir.path().join("home").join("flows") };
        let store = Store::new(paths, Some(&dir.path().join("project")));
        store.save(Scope::User, "news", Format::Json, "{\"flow\":\"cluaiz.flow/1\",\"name\":\"news\",\"steps\":[]}").unwrap();
        store.save(Scope::Project, "news", Format::Yaml, "flow: cluaiz.flow/1\nname: news\nsteps: []\n").unwrap();
        assert_eq!(store.find("news", None).unwrap().scope, Scope::Project);
        assert_eq!(store.find("news", Some(Scope::User)).unwrap().format, Format::Json);
        store.save(Scope::User, "news", Format::Yaml, "flow: cluaiz.flow/1\nname: news\nsteps: []\n").unwrap();
        assert_eq!(store.list(Some(Scope::User)).len(), 1);
    }

    #[test]
    fn permissions_are_written_into_the_text() {
        let text = "# my flow\nflow: cluaiz.flow/1\nname: n\nsteps: [{ set: {} }]\n";
        let flow = parse_text(text, Format::Yaml).unwrap();
        let out = with_permissions(text, Format::Yaml, &flow, &json!({ "tools": ["mcp:x/*"] }));
        assert!(out.starts_with("# my flow\n"), "{out}");
        assert_eq!(parse_text(&out, Format::Yaml).unwrap()["permissions"], json!({ "tools": ["mcp:x/*"] }));
        // An existing block is replaced where it is; the comments around it stay
        let text = "# top\nflow: cluaiz.flow/1\nname: n\npermissions:\n  folders: [\"~/a\"]\n# steps follow\nsteps: [{ set: {} }]\n";
        let flow = parse_text(text, Format::Yaml).unwrap();
        let wanted = json!({ "folders": ["~/a"], "methods": ["fs.read"] });
        let out = with_permissions(text, Format::Yaml, &flow, &wanted);
        assert!(out.contains("# top") && out.contains("# steps follow"), "{out}");
        assert_eq!(parse_text(&out, Format::Yaml).unwrap()["permissions"], wanted);
    }
}
