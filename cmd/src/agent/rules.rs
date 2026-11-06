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
    /// As /rules shows it: `~/.cluaiz/config/agent.json`, `.agents/settings.json`, `.agents/settings.local.json`
    pub label: String,
    pub settings: Project,
}

/// The project's settings file, shared in its repository, in the project's agent folder
pub const PROJECT_FILE: &str = "settings.json";
/// The user's own settings for the project, kept out of git
pub const LOCAL_FILE: &str = "settings.local.json";

/// The project's `.agents` folder: at the repository root, else in the working folder
pub fn agents_folder(dir: &std::path::Path) -> std::path::PathBuf {
    cluaiz_agents::workspace::project_folders(dir).0.join(".agents")
}

/// The agent settings of a working folder, as Claude Code layers its settings files:
///
/// 1. `~/.cluaiz/config/agent.json`: the user's rules for every folder
/// 2. `<project>/.agents/settings.json`: the project's, shared in its repository
/// 3. `<project>/.agents/settings.local.json`: the user's own for this project, kept out of git
///
/// `<project>` is the repository root (else the working folder), where the project's `.agents/`
/// folder is. The files of the first version, `<folder>/.cluaiz/agent.json` and
/// `agent.local.json`, are still read, before the new ones, with a notice to move them.
///
/// Rules of every file apply (a deny wins over any allow); the mode of the nearest file that sets
/// one is used. The global mode is the App's security mode, so the first file sets no mode.
#[derive(Default)]
pub struct Settings {
    pub layers: Vec<Layer>,
    /// Files that could not be used, and why (shown when the agent starts and in /rules)
    pub problems: Vec<String>,
    /// Every place a settings file is read from, with its label, whether or not it is there
    pub places: Vec<(String, std::path::PathBuf)>,
}

impl Settings {
    pub fn load(cluaiz_home: Option<&std::path::Path>, dir: &std::path::Path) -> Self {
        let display = |path: &std::path::Path| cluaiz_api::cep::roots::display(path);
        let agents = agents_folder(dir);
        // (file, label, where it moved to: Some for a file of the first version)
        let mut files: Vec<(std::path::PathBuf, String, Option<std::path::PathBuf>)> = Vec::new();
        if let Some(home) = cluaiz_home {
            files.push((home.join("config").join("agent.json"), "~/.cluaiz/config/agent.json".into(), None));
        }
        for (old, new) in [("agent.json", PROJECT_FILE), ("agent.local.json", LOCAL_FILE)] {
            files.push((dir.join(".cluaiz").join(old), format!(".cluaiz/{old}"), Some(agents.join(new))));
        }
        for name in [PROJECT_FILE, LOCAL_FILE] {
            files.push((agents.join(name), format!(".agents/{name}"), None));
        }

        let mut settings = Settings::default();
        // Old files are shown only when they are there
        settings.places = files
            .iter()
            .filter(|(path, _, moved)| moved.is_none() || path.is_file())
            .map(|(path, label, _)| (label.clone(), path.clone()))
            .collect();
        for (index, (path, label, moved)) in files.into_iter().enumerate() {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            if let Some(new) = &moved {
                settings.problems.push(format!(
                    "{} is read from its old place; move it to {}",
                    display(&path),
                    display(new)
                ));
            }
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

    /// What /rules shows: each settings file with its full path, then the rules in use
    pub fn summary(&self, rules: &Rules) -> String {
        let mut lines = vec!["Agent settings files (rules of all of them apply; the nearest mode is used):".to_string()];
        let width = self.places.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
        for (label, path) in &self.places {
            let state = match self.layers.iter().find(|layer| layer.label == *label) {
                Some(layer) => {
                    let mut parts = Vec::new();
                    if let Some(mode) = layer.settings.mode {
                        parts.push(format!("mode {}", mode.label()));
                    }
                    parts.push(format!("{} allow, {} deny", layer.settings.allow.len(), layer.settings.deny.len()));
                    parts.join(", ")
                }
                None if path.exists() => "not used, see below".to_string(),
                None => "not there yet".to_string(),
            };
            lines.push(format!("  {label:<width$}  {state}"));
            lines.push(format!("  {:<width$}  {}", "", cluaiz_api::cep::roots::display(path)));
        }
        for problem in &self.problems {
            lines.push(format!("! {problem}"));
        }
        let shown = rules.show();
        lines.push(String::new());
        if shown.is_empty() {
            lines.push(
                "No rules yet. Answer \"Yes, and always allow ... in this folder\" when the agent asks, and the \
                 rule is saved in .agents/settings.local.json; or write a file: {\"allow\": [\"run_command(cargo test*)\"], \
                 \"deny\": [\"run_command(git push*)\"]}"
                    .into(),
            );
        } else {
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

    /// The rules of a settings file, one per list item (`source` names it in a refusal and in /rules)
    pub fn add(&mut self, source: &str, allowed: &[String], denied: &[String]) {
        self.allowed.extend(allowed.iter().filter(|rule| !rule.trim().is_empty()).map(|rule| parse_one(rule, source)));
        self.denied.extend(denied.iter().filter(|rule| !rule.trim().is_empty()).map(|rule| parse_one(rule, source)));
    }

    /// `--allowed-tools` / `--disallowed-tools`: comma-separated rules
    pub fn add_flags(&mut self, source: &str, allowed: &[String], denied: &[String]) {
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

    /// An allow rule exists for this tool, with or without a pattern
    pub fn names(&self, tool: &str) -> bool {
        self.allowed.iter().any(|rule| rule.tool == tool)
    }

    /// What the rules of a call match: a command as the model wrote it (without the `in <folder>`
    /// line the approval adds), the file of a change relative to the working folder, else the first
    /// line of the approval text
    fn target(&self, tool: &str, what: &str) -> String {
        match tool {
            "run_command" => match what.rsplit_once('\n') {
                Some((command, last)) if last.starts_with("in ") => command.trim().to_owned(),
                _ => what.trim().to_owned(),
            },
            "write_file" | "edit_file" => {
                // "create src/a.rs" / "edit src/a.rs"
                let first = what.lines().next().unwrap_or_default().trim();
                let path = first.split_once(' ').map_or(first, |(_, path)| path).replace('\\', "/");
                path.strip_prefix(&self.root).map(str::to_owned).unwrap_or(path)
            }
            _ => what.lines().next().unwrap_or_default().trim().to_owned(),
        }
    }

    /// `what` is the approval text of the call
    pub fn check(&self, tool: &str, what: &str) -> Verdict {
        let target = self.target(tool, what);
        // A deny rule matches the whole command or any command chained in it (`a && git push`)
        let mut parts = vec![target.as_str()];
        if tool == "run_command" {
            parts.extend(target.split(['\n', ';', '|', '&']).map(str::trim).filter(|part| !part.is_empty()));
        }
        if let Some(rule) = self.denied.iter().find(|rule| parts.iter().any(|part| rule.matches(tool, part))) {
            return Verdict::Deny(format!("refused by the rule {} in {}", rule.show(), rule.source));
        }
        // An allow rule covers one plain command: a chained one, or one that redirects or runs
        // another command inside it, asks (`git status*` must not allow `git status; rm -r x`)
        if tool == "run_command" && chained(&target) {
            return Verdict::Ask;
        }
        if self.allowed.iter().any(|rule| rule.matches(tool, &target)) {
            return Verdict::Allow;
        }
        Verdict::Ask
    }

    /// The rule "always allow in this folder" saves for this call, or None when no rule should
    /// cover it (a chained command). A command with a sub-command keeps its first two words
    /// (`git status*`, `cargo test*`); any other command and every other call are saved exactly.
    pub fn suggest(&self, tool: &str, what: &str) -> Option<String> {
        let target = self.target(tool, what);
        if target.is_empty() || target.contains(['(', ')', '*']) {
            return None;
        }
        if tool != "run_command" {
            return Some(format!("{tool}({target})"));
        }
        if chained(&target) {
            return None;
        }
        let words: Vec<&str> = target.split_whitespace().collect();
        let sub_command = words.get(1).is_some_and(|word| {
            word.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        });
        Some(if sub_command {
            format!("run_command({} {}*)", words[0], words[1])
        } else {
            format!("run_command({target})")
        })
    }

    /// `rule` allowed from now on, as the settings file it was saved in
    pub fn allow(&mut self, rule: &str, source: &str) {
        self.allowed.push(parse_one(rule, source));
    }
}

/// A command that runs more than one command or redirects: `;`, `&&`, `||`, `|`, `&`, `>`, `<`,
/// `` ` ``, `$(` or a new line
fn chained(command: &str) -> bool {
    command.contains(['\n', ';', '|', '&', '>', '<', '`']) || command.contains("$(")
}

/// Saves `rule` in the allow list of `file` (a settings file as `Settings::load` reads it), keeping
/// everything else in it. A file that is not valid JSON is left as it is.
pub fn save_allow(file: &std::path::Path, rule: &str) -> Result<(), String> {
    let mut value: serde_json::Value = match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{} is not valid JSON ({error}); nothing was saved", file.display()))?,
        Err(_) => serde_json::json!({}),
    };
    let object = value.as_object_mut().ok_or_else(|| format!("{} is not a JSON object; nothing was saved", file.display()))?;
    let allow = object.entry("allow").or_insert_with(|| serde_json::json!([]));
    let list = allow.as_array_mut().ok_or_else(|| format!("\"allow\" in {} is not a list; nothing was saved", file.display()))?;
    if !list.iter().any(|item| item.as_str() == Some(rule)) {
        list.push(serde_json::Value::String(rule.to_owned()));
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    let new = !file.exists();
    std::fs::write(file, text + "\n").map_err(|error| format!("{}: {error}", file.display()))?;
    if new {
        keep_out_of_git(file);
    }
    Ok(())
}

/// A new local settings file is the user's own: like Claude Code's settings.local.json, git is
/// told to ignore it in this clone only (`.git/info/exclude`), so the repository's files stay as
/// they are
fn keep_out_of_git(file: &std::path::Path) {
    let Some(folder) = file.parent().and_then(|cluaiz| cluaiz.parent()) else { return };
    let (repo, _) = cluaiz_agents::workspace::project_folders(folder);
    let git = repo.join(".git");
    if !git.is_dir() {
        return;
    }
    let Ok(relative) = file.strip_prefix(repo) else { return };
    let line = format!("/{}", relative.display().to_string().replace('\\', "/"));
    let exclude = git.join("info").join("exclude");
    let current = std::fs::read_to_string(&exclude).unwrap_or_default();
    if current.lines().any(|existing| existing.trim() == line) {
        return;
    }
    let mut text = current;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&line);
    text.push('\n');
    if std::fs::create_dir_all(git.join("info")).is_ok() {
        let _ = std::fs::write(exclude, text);
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

/// One rule: `tool` or `tool(pattern)`
fn parse_one(text: &str, source: &str) -> Rule {
    let text = text.trim();
    match text.split_once('(') {
        Some((tool, rest)) => Rule {
            tool: tool.trim().into(),
            pattern: Some(rest.strip_suffix(')').unwrap_or(rest).trim().into()),
            source: source.into(),
        },
        None => Rule { tool: text.into(), pattern: None, source: source.into() },
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
                    rules.push(parse_one(text, source));
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
    fn an_allow_rule_names_its_tool() {
        let mut rules = Rules::new(std::path::Path::new("C:\\w"));
        rules.add_flags("--allowed-tools", &["mcp:files,plugin:calc(1*)".into()], &["mcp:web".into()]);
        assert!(rules.names("mcp:files"));
        assert!(rules.names("plugin:calc"));
        assert!(!rules.names("mcp:web"));
        assert!(!rules.names("mcp:other"));
    }

    #[test]
    fn rules_allow_deny_and_ask() {
        let mut rules = Rules::new(std::path::Path::new("C:\\w"));
        rules.add_flags("--allowed-tools", &["read_file,run_command(git *),edit_file(src/*)".into()], &["run_command(git push*)".into()]);
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
        std::fs::create_dir_all(root.join(".agents")).unwrap();
        std::fs::write(home.join("config/agent.json"), r#"{"mode": "full", "deny": ["run_command(git push*)"]}"#).unwrap();
        std::fs::write(root.join(".agents/settings.json"), r#"{"mode": "plan", "allow": ["run_command(cargo test*)"]}"#).unwrap();
        std::fs::write(root.join(".agents/settings.local.json"), r#"{"mode": "accept-edits", "allow": ["run_command(git *)"]}"#).unwrap();

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
        assert!(settings.summary(&rules).contains("[.agents/settings.local.json]"));

        // A broken file is reported, not silently ignored
        std::fs::write(root.join(".agents/settings.local.json"), "{ not json").unwrap();
        let settings = Settings::load(Some(&home), &root);
        assert_eq!(settings.layers.len(), 2);
        assert!(settings.problems.iter().any(|problem| problem.contains("settings.local.json is not used")));
        assert_eq!(settings.mode(), Some(super::super::tools::Mode::Plan));

        // A file of the first version is still read, with a notice where it goes now; the new
        // project file comes after it, so its mode wins
        std::fs::create_dir_all(root.join(".cluaiz")).unwrap();
        std::fs::write(root.join(".cluaiz/agent.json"), r#"{"mode": "full", "deny": ["edit_file(.env)"]}"#).unwrap();
        let settings = Settings::load(Some(&home), &root);
        assert!(settings.problems.iter().any(|problem| problem.contains("old place") && problem.contains("settings.json")));
        assert!(settings.layers.iter().any(|layer| layer.label == ".cluaiz/agent.json"));
        assert_eq!(settings.mode(), Some(super::super::tools::Mode::Plan));
        assert!(settings.places.iter().any(|(label, _)| label == ".cluaiz/agent.json"));
        assert!(!settings.places.iter().any(|(label, _)| label == ".cluaiz/agent.local.json"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn chained_commands_are_not_allowed_by_a_rule_and_deny_finds_any_part() {
        let mut rules = Rules::new(std::path::Path::new("C:\\w"));
        rules.add(".cluaiz/agent.json", &["run_command(git status*)".into()], &["run_command(git push*)".into()]);
        assert!(matches!(rules.check("run_command", "git status --short\nin C:/w"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "git status; rm -r src\nin C:/w"), Verdict::Ask));
        assert!(matches!(rules.check("run_command", "git status\nrm -r src\nin C:/w"), Verdict::Ask));
        assert!(matches!(rules.check("run_command", "git status > out.txt\nin C:/w"), Verdict::Ask));
        assert!(matches!(rules.check("run_command", "git add . && git push origin\nin C:/w"), Verdict::Deny(_)));
    }

    #[test]
    fn always_allow_suggests_a_narrow_rule_and_saves_it() {
        let rules = Rules::new(std::path::Path::new("C:\\w"));
        assert_eq!(rules.suggest("run_command", "git status --short\nin C:/w").as_deref(), Some("run_command(git status*)"));
        assert_eq!(rules.suggest("run_command", "cargo test -p cmd\nin C:/w").as_deref(), Some("run_command(cargo test*)"));
        // No sub-command: only this exact command
        assert_eq!(rules.suggest("run_command", "python build.py\nin C:/w").as_deref(), Some("run_command(python build.py)"));
        assert_eq!(rules.suggest("run_command", "ls -la\nin C:/w").as_deref(), Some("run_command(ls -la)"));
        assert_eq!(rules.suggest("run_command", "git status && git push\nin C:/w"), None);
        assert_eq!(rules.suggest("edit_file", "edit C:\\w\\src\\lib.rs\n- a\n+ b").as_deref(), Some("edit_file(src/lib.rs)"));

        let base = std::env::temp_dir().join(format!("cluaiz-agent-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let file = base.join(".agents").join("settings.local.json");
        save_allow(&file, "run_command(git status*)").unwrap();
        save_allow(&file, "run_command(git status*)").unwrap();
        std::fs::write(&file, r#"{"mode": "plan", "allow": ["run_command(git status*)"], "deny": ["x"]}"#).unwrap();
        save_allow(&file, "edit_file(src/lib.rs)").unwrap();
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved["allow"], serde_json::json!(["run_command(git status*)", "edit_file(src/lib.rs)"]));
        assert_eq!(saved["mode"], "plan");
        assert_eq!(saved["deny"], serde_json::json!(["x"]));
        // A broken file is not overwritten
        std::fs::write(&file, "{ broken").unwrap();
        assert!(save_allow(&file, "read_file").is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ broken");
        let _ = std::fs::remove_dir_all(base);
    }
}
