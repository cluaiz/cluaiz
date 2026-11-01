//! `--allowed-tools` / `--disallowed-tools` rules, as `claude -p` has them: `read_file`, or a tool
//! with a pattern for what it works on, `run_command(git *)`, `edit_file(src/*)`. `*` matches any
//! text. A denied call is refused before anything asks; an allowed one runs without asking.

/// One agent settings file: `{"mode": "accept-edits", "allow": ["run_command(cargo test*)"],
/// "deny": ["run_command(git push*)"]}`
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Project {
    pub mode: Option<super::tools::Mode>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// One settings file that was read, and what it said
pub struct Layer {
    /// As /rules shows it: `~/.cluaiz/config/agent.json`, `.cluaiz/agent.json`, `.cluaiz/agent.local.json`
    pub label: String,
    pub settings: Project,
}

/// The agent settings of a working folder, as Claude Code layers its settings files:
///
/// 1. `~/.cluaiz/config/agent.json`: the user's rules for every folder
/// 2. `<folder>/.cluaiz/agent.json`: the project's, shared in its repository
/// 3. `<folder>/.cluaiz/agent.local.json`: the user's own for this folder, kept out of git
///
/// Rules of every file apply (a deny wins over any allow); the mode of the nearest file that sets
/// one is used. The global mode is the App's security mode, so the first file sets no mode.
#[derive(Default)]
pub struct Settings {
    pub layers: Vec<Layer>,
    /// Files that could not be used, and why (shown when the agent starts and in /rules)
    pub problems: Vec<String>,
}

impl Settings {
    pub fn load(cluaiz_home: Option<&std::path::Path>, root: &std::path::Path) -> Self {
        let mut files = Vec::new();
        if let Some(home) = cluaiz_home {
            files.push((home.join("config").join("agent.json"), "~/.cluaiz/config/agent.json"));
        }
        files.push((root.join(".cluaiz").join("agent.json"), ".cluaiz/agent.json"));
        files.push((root.join(".cluaiz").join("agent.local.json"), ".cluaiz/agent.local.json"));

        let mut settings = Settings::default();
        for (index, (path, label)) in files.into_iter().enumerate() {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let mut parsed: Project = match serde_json::from_str(&text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    settings.problems.push(format!("{} is not used: {error}", cluaiz_api::cep::roots::display(&path)));
                    continue;
                }
            };
            if index == 0 && cluaiz_home.is_some() && parsed.mode.take().is_some() {
                settings.problems.push(format!(
                    "\"mode\" in {} is not used: the mode for every folder is the App's security mode (/mode); \
                     its rules are used",
                    cluaiz_api::cep::roots::display(&path)
                ));
            }
            settings.layers.push(Layer { label: label.into(), settings: parsed });
        }
        settings
    }

    /// The mode of the nearest file that sets one
    pub fn mode(&self) -> Option<super::tools::Mode> {
        self.layers.iter().rev().find_map(|layer| layer.settings.mode)
    }

    /// What /rules shows
    pub fn summary(&self, rules: &Rules) -> String {
        let mut lines = Vec::new();
        if self.layers.is_empty() {
            lines.push(
                "No agent settings files. Rules for every folder go in ~/.cluaiz/config/agent.json, for this \
                 project in .cluaiz/agent.json, and your own for this folder in .cluaiz/agent.local.json:\n  {\"allow\": [\"run_command(cargo test*)\"], \"deny\": [\"run_command(git push*)\"]}"
                    .to_string(),
            );
        }
        for layer in &self.layers {
            let mut parts = Vec::new();
            if let Some(mode) = layer.settings.mode {
                parts.push(format!("mode {}", mode.label()));
            }
            parts.push(format!("{} allow, {} deny", layer.settings.allow.len(), layer.settings.deny.len()));
            lines.push(format!("{}: {}", layer.label, parts.join(", ")));
        }
        for problem in &self.problems {
            lines.push(format!("! {problem}"));
        }
        let shown = rules.show();
        if !shown.is_empty() {
            lines.push(String::new());
            lines.push(shown);
        }
        lines.join("\n")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rules {
    allowed: Vec<Rule>,
    denied: Vec<Rule>,
    /// The working folder: file patterns are relative to it
    root: String,
}

#[derive(Clone, Debug)]
struct Rule {
    tool: String,
    pattern: Option<String>,
    /// Where it was written: a settings file, or `--allowed-tools` / `--disallowed-tools`
    source: String,
}

pub enum Verdict {
    Allow,
    Deny(String),
    Ask,
}

impl Rules {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            allowed: Vec::new(),
            denied: Vec::new(),
            // Shown without the `\\?\` prefix of a canonical Windows path, as approvals show files
            root: cluaiz_api::cep::roots::display(root).replace('\\', "/").trim_end_matches('/').to_owned() + "/",
        }
    }

    /// Rules from one place (`source` names it in a refusal and in /rules)
    pub fn add(&mut self, source: &str, allowed: &[String], denied: &[String]) {
        self.allowed.extend(allowed.iter().flat_map(|list| parse(list, source)));
        self.denied.extend(denied.iter().flat_map(|list| parse(list, source)));
    }

    /// The rules in use, with where each was written
    pub fn show(&self) -> String {
        let mut lines = Vec::new();
        for (title, rules) in [("Allowed without asking:", &self.allowed), ("Always refused:", &self.denied)] {
            if !rules.is_empty() {
                lines.push(title.to_string());
                lines.extend(rules.iter().map(|rule| format!("  {}  [{}]", rule.show(), rule.source)));
            }
        }
        lines.join("\n")
    }

    /// `what` is the approval text of the call: its first line names the command or the file
    pub fn check(&self, tool: &str, what: &str) -> Verdict {
        let first = what.lines().next().unwrap_or_default().trim();
        // "create src/a.rs" / "edit src/a.rs": the file; a command line as it is
        let target = match tool {
            "write_file" | "edit_file" => {
                let path = first.split_once(' ').map_or(first, |(_, path)| path).replace('\\', "/");
                path.strip_prefix(&self.root).map(str::to_owned).unwrap_or(path)
            }
            _ => first.to_owned(),
        };
        let target = target.as_str();
        if let Some(rule) = self.denied.iter().find(|rule| rule.matches(tool, target)) {
            return Verdict::Deny(format!("refused by the rule {} in {}", rule.show(), rule.source));
        }
        if self.allowed.iter().any(|rule| rule.matches(tool, target)) {
            return Verdict::Allow;
        }
        Verdict::Ask
    }
}

impl Rule {
    fn matches(&self, tool: &str, target: &str) -> bool {
        self.tool == tool && self.pattern.as_deref().is_none_or(|pattern| glob(pattern, target))
    }

    fn show(&self) -> String {
        match &self.pattern {
            Some(pattern) => format!("{}({pattern})", self.tool),
            None => self.tool.clone(),
        }
    }
}

/// `a,b(x, y),c`: commas inside parentheses belong to the pattern
fn parse(list: &str, source: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut depth = 0;
    let mut item = String::new();
    for c in list.chars().chain([',']) {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let text = std::mem::take(&mut item);
                let text = text.trim();
                if !text.is_empty() {
                    rules.push(match text.split_once('(') {
                        Some((tool, rest)) => Rule { tool: tool.trim().into(), pattern: Some(rest.trim_end_matches(')').trim().into()), source: source.into() },
                        None => Rule { tool: text.into(), pattern: None, source: source.into() },
                    });
                }
                continue;
            }
            _ => {}
        }
        item.push(c);
    }
    rules
}

/// `*` matches any text; everything else matches itself (case-sensitive, like the shell)
fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(after) => rest = after,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_allow_deny_and_ask() {
        let mut rules = Rules::new(std::path::Path::new("C:\\w"));
        rules.add("--allowed-tools", &["read_file,run_command(git *),edit_file(src/*)".into()], &["run_command(git push*)".into()]);
        assert!(matches!(rules.check("edit_file", "edit C:\\w\\src\\lib.rs"), Verdict::Allow));
        assert!(matches!(rules.check("read_file", "read a.txt"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "git status\nin /w"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "git push origin\nin /w"), Verdict::Deny(_)));
        assert!(matches!(rules.check("run_command", "rm -r x\nin /w"), Verdict::Ask));
        assert!(matches!(rules.check("edit_file", "edit src/main.rs\n- a\n+ b"), Verdict::Allow));
        assert!(matches!(rules.check("edit_file", "edit docs/a.md"), Verdict::Ask));
    }

    #[test]
    fn settings_files_are_layered_and_bad_ones_are_reported() {
        let base = std::env::temp_dir().join(format!("cluaiz-agent-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let root = base.join("project");
        std::fs::create_dir_all(home.join("config")).unwrap();
        std::fs::create_dir_all(root.join(".cluaiz")).unwrap();
        std::fs::write(home.join("config/agent.json"), r#"{"mode": "full", "deny": ["run_command(git push*)"]}"#).unwrap();
        std::fs::write(root.join(".cluaiz/agent.json"), r#"{"mode": "plan", "allow": ["run_command(cargo test*)"]}"#).unwrap();
        std::fs::write(root.join(".cluaiz/agent.local.json"), r#"{"mode": "accept-edits", "allow": ["run_command(git *)"]}"#).unwrap();

        let settings = Settings::load(Some(&home), &root);
        assert_eq!(settings.layers.len(), 3);
        // The nearest mode wins; the global file's mode is the App's and is reported, not used
        assert_eq!(settings.mode(), Some(super::super::tools::Mode::AcceptEdits));
        assert_eq!(settings.problems.len(), 1);
        assert!(settings.problems[0].contains("\"mode\""));

        let mut rules = Rules::new(&root);
        for layer in &settings.layers {
            rules.add(&layer.label, &layer.settings.allow, &layer.settings.deny);
        }
        // The global deny wins over the local allow
        match rules.check("run_command", "git push origin\nin /w") {
            Verdict::Deny(reason) => assert!(reason.contains("~/.cluaiz/config/agent.json")),
            _ => panic!("git push must be refused"),
        }
        assert!(matches!(rules.check("run_command", "git status\nin /w"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "cargo test -p cmd\nin /w"), Verdict::Allow));
        assert!(settings.summary(&rules).contains("[.cluaiz/agent.local.json]"));

        // A broken file is reported, not silently ignored
        std::fs::write(root.join(".cluaiz/agent.local.json"), "{ not json").unwrap();
        let settings = Settings::load(Some(&home), &root);
        assert_eq!(settings.layers.len(), 2);
        assert!(settings.problems.iter().any(|problem| problem.contains("agent.local.json is not used")));
        assert_eq!(settings.mode(), Some(super::super::tools::Mode::Plan));
        let _ = std::fs::remove_dir_all(base);
    }
}
