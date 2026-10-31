//! `--allowed-tools` / `--disallowed-tools` rules, as `claude -p` has them: `read_file`, or a tool
//! with a pattern for what it works on, `run_command(git *)`, `edit_file(src/*)`. `*` matches any
//! text. A denied call is refused before anything asks; an allowed one runs without asking.

/// A folder's agent permissions in `.cluaiz/agent.json`:
/// `{"mode": "accept-edits", "allow": ["run_command(cargo test*)"], "deny": ["run_command(git push*)"]}`
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Project {
    pub mode: Option<super::tools::Mode>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

impl Project {
    pub fn load(root: &std::path::Path) -> Self {
        std::fs::read_to_string(root.join(".cluaiz").join("agent.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
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
}

pub enum Verdict {
    Allow,
    Deny(String),
    Ask,
}

impl Rules {
    pub fn new(allowed: &[String], denied: &[String], root: &std::path::Path) -> Self {
        Self {
            allowed: allowed.iter().flat_map(|list| parse(list)).collect(),
            denied: denied.iter().flat_map(|list| parse(list)).collect(),
            // Shown without the `\\?\` prefix of a canonical Windows path, as approvals show files
            root: cluaiz_api::cep::roots::display(root).replace('\\', "/").trim_end_matches('/').to_owned() + "/",
        }
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
            return Verdict::Deny(format!("refused by --disallowed-tools {}", rule.show()));
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
fn parse(list: &str) -> Vec<Rule> {
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
                        Some((tool, rest)) => Rule { tool: tool.trim().into(), pattern: Some(rest.trim_end_matches(')').trim().into()) },
                        None => Rule { tool: text.into(), pattern: None },
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
        let rules = Rules::new(&["read_file,run_command(git *),edit_file(src/*)".into()], &["run_command(git push*)".into()], std::path::Path::new("C:\\w"));
        assert!(matches!(rules.check("edit_file", "edit C:\\w\\src\\lib.rs"), Verdict::Allow));
        assert!(matches!(rules.check("read_file", "read a.txt"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "git status\nin /w"), Verdict::Allow));
        assert!(matches!(rules.check("run_command", "git push origin\nin /w"), Verdict::Deny(_)));
        assert!(matches!(rules.check("run_command", "rm -r x\nin /w"), Verdict::Ask));
        assert!(matches!(rules.check("edit_file", "edit src/main.rs\n- a\n+ b"), Verdict::Allow));
        assert!(matches!(rules.check("edit_file", "edit docs/a.md"), Verdict::Ask));
    }
}
