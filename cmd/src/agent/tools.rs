//! The agent's built-in tools: read, list and search files of the working folder, write and edit
//! files, and run shell commands. Reads inside the folder run at once; a read outside it, every file
//! change and every command that is not read-only ask the user first (`--yes` allows them all).
//! Commands go through the same policy as the Engine's terminal tool (`cluaiz_agents::ExecutionPolicy`):
//! destructive patterns are refused even with `--yes`.

use std::collections::HashSet;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;

use cluaiz_agents::{CancelHandle, ExecutionPolicy, PolicyDecision, SandboxTerminalRunner, SecurityMode};
use colored::Colorize;
use serde_json::{json, Value};

/// Most characters of one tool result the model gets (a small context fills fast)
const MAX_RESULT: usize = 16_000;
const READ_LINES: usize = 400;
const SEARCH_HITS: usize = 100;
const LIST_ENTRIES: usize = 300;
/// Folders a search never walks into
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "venv", "__pycache__", "dist", "build", ".next"];

pub struct Tools {
    root: PathBuf,
    /// `--yes`: no questions
    pub allow_all: bool,
    /// Tools the user allowed for the rest of the session ("always")
    always: HashSet<String>,
}

impl Tools {
    pub fn new(root: PathBuf, allow_all: bool) -> Self {
        Self { root, allow_all, always: HashSet::new() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Function schemas, as an API request's `tools`
    pub fn schemas() -> Value {
        let function = |name: &str, description: &str, properties: Value, required: &[&str]| {
            json!({ "type": "function", "function": {
                "name": name,
                "description": description,
                "parameters": { "type": "object", "properties": properties, "required": required }
            }})
        };
        json!([
            function(
                "read_file",
                "Read a text file. Lines come numbered. Paths are relative to the working folder.",
                json!({
                    "path": { "type": "string", "description": "File path" },
                    "offset": { "type": "integer", "description": "First line to read, from 1 (default 1)" },
                    "limit": { "type": "integer", "description": "Most lines to read (default 400)" }
                }),
                &["path"],
            ),
            function(
                "list_dir",
                "List the files and folders in a folder (folders end with /).",
                json!({ "path": { "type": "string", "description": "Folder path (default: the working folder)" } }),
                &[],
            ),
            function(
                "search",
                "Search the text of files for a regular expression. Returns path:line: text for each match.",
                json!({
                    "pattern": { "type": "string", "description": "Regular expression, e.g. fn main or (?i)todo" },
                    "path": { "type": "string", "description": "File or folder to search (default: the working folder)" },
                    "glob": { "type": "string", "description": "Only file names like this, e.g. *.rs" }
                }),
                &["pattern"],
            ),
            function(
                "write_file",
                "Create a file or replace all of its content.",
                json!({
                    "path": { "type": "string", "description": "File path" },
                    "content": { "type": "string", "description": "The whole new content" }
                }),
                &["path", "content"],
            ),
            function(
                "edit_file",
                "Replace text in a file. old_text must appear exactly once (with its spaces and line breaks) unless replace_all is true. Read the file first.",
                json!({
                    "path": { "type": "string", "description": "File path" },
                    "old_text": { "type": "string", "description": "Exact text to replace" },
                    "new_text": { "type": "string", "description": "Text to put in its place" },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence (default false)" }
                }),
                &["path", "old_text", "new_text"],
            ),
            function(
                "run_command",
                if cfg!(windows) { "Run a PowerShell command in the working folder. Returns the exit code and output." } else { "Run a shell (sh) command in the working folder. Returns the exit code and output." },
                json!({
                    "command": { "type": "string", "description": "The command line" },
                    "timeout": { "type": "integer", "description": "Seconds before it is stopped (default 120, most 600)" }
                }),
                &["command"],
            ),
        ])
    }

    /// Runs one call and returns what the model gets back. The call's row and a one-line summary are
    /// printed on stderr.
    pub async fn run(&mut self, name: &str, arguments: &str) -> String {
        let args: Value = match serde_json::from_str(arguments) {
            Ok(v @ Value::Object(_)) => v,
            _ => return format!("Error: the arguments are not a JSON object: {arguments}"),
        };
        let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_string);
        let number = |key: &str| args.get(key).and_then(Value::as_u64).map(|n| n as usize);

        let result = match name {
            "read_file" => match text("path") {
                Some(path) => self.read_file(&path, number("offset"), number("limit")),
                None => Err("give a path".into()),
            },
            "list_dir" => self.list_dir(&text("path").unwrap_or_else(|| ".".into())),
            "search" => match text("pattern") {
                Some(pattern) => self.search(&pattern, &text("path").unwrap_or_else(|| ".".into()), text("glob")),
                None => Err("give a pattern".into()),
            },
            "write_file" => match (text("path"), text("content")) {
                (Some(path), Some(content)) => self.write_file(&path, &content),
                _ => Err("give a path and content".into()),
            },
            "edit_file" => match (text("path"), text("old_text"), text("new_text")) {
                (Some(path), Some(old), Some(new)) => {
                    self.edit_file(&path, &old, &new, args.get("replace_all").and_then(Value::as_bool).unwrap_or(false))
                }
                _ => Err("give a path, old_text and new_text".into()),
            },
            "run_command" => match text("command") {
                Some(command) => self.run_command(&command, number("timeout").unwrap_or(120).clamp(1, 600)).await,
                None => Err("give a command".into()),
            },
            other => {
                row(other, "");
                Err(format!("there is no tool named '{other}'. Tools: read_file, list_dir, search, write_file, edit_file, run_command"))
            }
        };
        match result {
            Ok((summary, output)) => {
                done(&summary);
                clip(&output)
            }
            Err(e) => {
                done(&format!("{} {}", "error:".red(), e));
                format!("Error: {e}")
            }
        }
    }

    fn read_file(&mut self, path: &str, offset: Option<usize>, limit: Option<usize>) -> Result<(String, String), String> {
        let file = self.resolve(path);
        row("Read", &self.show(&file));
        self.allow_outside("read_file", &file)?;
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Err("a binary file, not text".into());
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let first = offset.unwrap_or(1).max(1);
        let limit = limit.unwrap_or(READ_LINES).max(1);
        let shown: Vec<String> =
            lines.iter().enumerate().skip(first - 1).take(limit).map(|(i, l)| format!("{:>5}\t{}", i + 1, l)).collect();
        let mut out = shown.join("\n");
        let last = first - 1 + shown.len();
        if last < lines.len() {
            out.push_str(&format!("\n({} more lines: read with offset {})", lines.len() - last, last + 1));
        }
        if lines.is_empty() {
            out = "(empty file)".into();
        }
        Ok((format!("{} lines", shown.len()), out))
    }

    fn list_dir(&mut self, path: &str) -> Result<(String, String), String> {
        let dir = self.resolve(path);
        row("List", &self.show(&dir));
        self.allow_outside("list_dir", &dir)?;
        let mut entries: Vec<(bool, String)> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", self.show(&dir)))?
            .flatten()
            .map(|e| (e.path().is_dir(), e.file_name().to_string_lossy().to_string()))
            .collect();
        entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        let total = entries.len();
        let mut out: Vec<String> = entries.into_iter().take(LIST_ENTRIES).map(|(d, n)| if d { format!("{n}/") } else { n }).collect();
        if total > LIST_ENTRIES {
            out.push(format!("({} more)", total - LIST_ENTRIES));
        }
        Ok((format!("{total} entries"), if out.is_empty() { "(empty folder)".into() } else { out.join("\n") }))
    }

    fn search(&mut self, pattern: &str, path: &str, glob: Option<String>) -> Result<(String, String), String> {
        let start = self.resolve(path);
        row("Search", &format!("{} in {}", pattern, self.show(&start)));
        self.allow_outside("search", &start)?;
        let re = regex::Regex::new(pattern).or_else(|_| regex::Regex::new(&regex::escape(pattern))).map_err(|e| e.to_string())?;
        let name_matches = |name: &str| match glob.as_deref() {
            Some(g) => match g.strip_prefix('*') {
                Some(suffix) => name.ends_with(suffix),
                None => name == g,
            },
            None => true,
        };
        let mut hits = Vec::new();
        let mut more = 0usize;
        let walker = walkdir::WalkDir::new(&start)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref())));
        for entry in walker.flatten() {
            if !entry.file_type().is_file() || !name_matches(&entry.file_name().to_string_lossy()) {
                continue;
            }
            if entry.metadata().map(|m| m.len() > 2_000_000).unwrap_or(true) {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            if bytes.iter().take(8000).any(|b| *b == 0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    if hits.len() < SEARCH_HITS {
                        let line: String = line.trim().chars().take(200).collect();
                        hits.push(format!("{}:{}: {}", self.show(entry.path()), i + 1, line));
                    } else {
                        more += 1;
                    }
                }
            }
        }
        let count = hits.len() + more;
        if more > 0 {
            hits.push(format!("({more} more matches: narrow the search)"));
        }
        Ok((format!("{count} matches"), if hits.is_empty() { "No matches.".into() } else { hits.join("\n") }))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(String, String), String> {
        let file = self.resolve(path);
        let exists = file.exists();
        row(if exists { "Write" } else { "Create" }, &self.show(&file));
        let lines = content.lines().count();
        for line in content.lines().take(12) {
            eprintln!("    {}", format!("+ {line}").green());
        }
        if lines > 12 {
            eprintln!("    {}", format!("… {} more lines", lines - 12).dimmed());
        }
        self.ask("write_file", "")?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        let what = if exists { "replaced" } else { "created" };
        Ok((format!("{what}, {lines} lines"), format!("{} {what} ({lines} lines).", self.show(&file))))
    }

    fn edit_file(&mut self, path: &str, old: &str, new: &str, replace_all: bool) -> Result<(String, String), String> {
        let file = self.resolve(path);
        row("Edit", &self.show(&file));
        let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        // A file with Windows line breaks matches text the model wrote with plain ones
        let (old, new) = if !text.contains(old) && text.contains("\r\n") {
            (old.replace("\r\n", "\n").replace('\n', "\r\n"), new.replace("\r\n", "\n").replace('\n', "\r\n"))
        } else {
            (old.to_string(), new.to_string())
        };
        if old.is_empty() {
            return Err("old_text is empty".into());
        }
        let found = text.matches(&old).count();
        if found == 0 {
            return Err("old_text is not in the file; read the file and copy the text exactly".into());
        }
        if found > 1 && !replace_all {
            return Err(format!("old_text is in the file {found} times; add the lines around it, or set replace_all"));
        }
        for line in old.lines().take(12) {
            eprintln!("    {}", format!("- {line}").red());
        }
        for line in new.lines().take(12) {
            eprintln!("    {}", format!("+ {line}").green());
        }
        self.ask("edit_file", "")?;
        let updated = if replace_all { text.replace(&old, &new) } else { text.replacen(&old, &new, 1) };
        std::fs::write(&file, updated).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        let n = if replace_all { found } else { 1 };
        Ok((format!("{n} replaced"), format!("{} edited ({n} replaced).", self.show(&file))))
    }

    async fn run_command(&mut self, command: &str, timeout: usize) -> Result<(String, String), String> {
        row("Run", command);
        let mode = match ExecutionPolicy::evaluate(command, &self.root, SecurityMode::RequireApproval) {
            PolicyDecision::Deny { reason } => return Err(format!("refused: {reason}")),
            PolicyDecision::Allow if !chains(command) => SecurityMode::RequireApproval,
            PolicyDecision::Allow | PolicyDecision::AskConfirmation { .. } => {
                self.ask("run_command", "")?;
                SecurityMode::AlwaysAllow
            }
        };
        // Ctrl+C stops the command (the handler sets the cancel signal while this runs)
        let (cancel, cancel_rx) = CancelHandle::new();
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
        crate::chat::GENERATING.store(true, Ordering::SeqCst);
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) {
                    cancel.cancel();
                    break;
                }
            }
        });
        let result = SandboxTerminalRunner::execute_advanced(command, &self.root, mode, None, None, Some(timeout as u64), Some(cancel_rx)).await;
        watcher.abort();
        crate::chat::GENERATING.store(false, Ordering::SeqCst);
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
        let run = result.map_err(|e| e.to_string())?;
        let mut out = format!("Exit code {}", run.exit_code);
        if run.cancelled {
            out.push_str(" (stopped)");
        }
        if let Some(info) = &run.signal_info {
            out.push_str(&format!(" ({info})"));
        }
        if !run.stdout.trim().is_empty() {
            out.push_str(&format!("\nstdout:\n{}", run.stdout.trim_end()));
        }
        if !run.stderr.trim().is_empty() {
            out.push_str(&format!("\nstderr:\n{}", run.stderr.trim_end()));
        }
        let lines = run.stdout.lines().count() + run.stderr.lines().count();
        let state = if run.cancelled { "stopped".to_string() } else { format!("exit {}", run.exit_code) };
        Ok((format!("{state}, {lines} lines of output"), out))
    }

    /// A path relative to the working folder, `..` resolved without touching the disk
    fn resolve(&self, path: &str) -> PathBuf {
        let joined = self.root.join(path.trim());
        let mut out = PathBuf::new();
        for part in joined.components() {
            match part {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    }

    /// The path as shown: relative inside the working folder
    fn show(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rel) if rel.as_os_str().is_empty() => ".".into(),
            Ok(rel) => rel.display().to_string().replace('\\', "/"),
            Err(_) => path.display().to_string(),
        }
    }

    fn allow_outside(&mut self, tool: &str, path: &Path) -> Result<(), String> {
        if path.starts_with(&self.root) {
            return Ok(());
        }
        self.ask(tool, "outside the working folder")
    }

    /// The user's answer to one call; Err is what the model is told when it is not allowed.
    fn ask(&mut self, tool: &str, why: &str) -> Result<(), String> {
        if self.allow_all || self.always.contains(tool) {
            return Ok(());
        }
        if !std::io::stdin().is_terminal() {
            return Err("not allowed: there is no terminal to ask the user (cluaiz agent --yes allows it)".into());
        }
        let why = if why.is_empty() { String::new() } else { format!(" ({why})") };
        let answer = tokio::task::block_in_place(|| {
            eprint!("  {}{} {} ", "Allow?".yellow().bold(), why, "[y]es / [n]o / [a]lways this session:".dimmed());
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            let _ = std::io::stdin().lock().read_line(&mut line);
            line.trim().to_lowercase()
        });
        match answer.as_str() {
            "y" | "yes" => Ok(()),
            "a" | "always" => {
                self.always.insert(tool.to_string());
                Ok(())
            }
            _ => Err("the user did not allow this; ask them what to do instead".into()),
        }
    }
}

/// The policy reads the start of a line: `ls; rm -r src` or `echo x > file` start read-only. A line
/// that chains, pipes or redirects asks like any other change.
fn chains(command: &str) -> bool {
    [">", "<", ";", "|", "&", "`", "$(", "\n"].iter().any(|c| command.contains(c))
}

/// `● Read src/main.rs`
fn row(action: &str, target: &str) {
    eprintln!("{} {} {}", "●".cyan(), action.bold(), target);
}

/// `  ⎿ 120 lines`
fn done(summary: &str) {
    eprintln!("  {} {}", "⎿".dimmed(), summary.dimmed());
}

/// A long result keeps its start and end
fn clip(text: &str) -> String {
    if text.len() <= MAX_RESULT {
        return text.to_string();
    }
    let mut head = MAX_RESULT * 3 / 4;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - MAX_RESULT / 4;
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}\n… ({} characters left out) …\n{}", &text[..head], tail - head, &text[tail..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> (Tools, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cluaiz-agent-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {\r\n    println!(\"hi\");\r\n}\r\n").unwrap();
        (Tools::new(dir.clone(), true), dir)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn files_are_read_searched_and_edited() {
        let (mut t, dir) = tools();
        let read = t.run("read_file", r#"{"path": "src/main.rs"}"#).await;
        assert!(read.starts_with("    1\tfn main() {"), "{read}");
        let found = t.run("search", r#"{"pattern": "println", "glob": "*.rs"}"#).await;
        assert_eq!(found, "src/main.rs:2: println!(\"hi\");");
        // Plain line breaks match the file's Windows ones
        let edit = t.run("edit_file", r#"{"path": "src/main.rs", "old_text": "{\n    println!(\"hi\");", "new_text": "{\n    println!(\"bye\");"}"#).await;
        assert_eq!(edit, "src/main.rs edited (1 replaced).");
        assert!(std::fs::read_to_string(dir.join("src/main.rs")).unwrap().contains("println!(\"bye\");\r\n"));
        let twice = t.run("edit_file", r#"{"path": "src/main.rs", "old_text": "nothing like this", "new_text": "x"}"#).await;
        assert!(twice.starts_with("Error: old_text is not in the file"));
        assert_eq!(t.run("list_dir", "{}").await, "src/");
        assert!(t.run("nope", "{}").await.starts_with("Error: there is no tool named 'nope'"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_only_start_does_not_cover_the_rest_of_the_line() {
        assert!(!chains("git status"));
        assert!(!chains("Get-ChildItem src"));
        for line in ["ls; rm -r src", "echo hi > notes.txt", "cat a | tee b", "dir && del x", "echo $(whoami)"] {
            assert!(chains(line), "{line}");
        }
    }

    #[test]
    fn paths_resolve_inside_the_folder() {
        let t = Tools::new(PathBuf::from("/work/app"), false);
        assert_eq!(t.resolve("src/../lib.rs"), PathBuf::from("/work/app/lib.rs"));
        assert!(!t.resolve("../../etc/passwd").starts_with("/work/app"));
    }
}
