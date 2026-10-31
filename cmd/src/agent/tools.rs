//! The agent's built-in tools: read, list and search files of the working folder, write and edit
//! files, and run shell commands. Reads inside the folder run at once; a read outside it, every file
//! change and every command that is not read-only ask the user first (`--yes` allows them all).
//! Commands go through the same policy as the Engine's terminal tool (`cluaiz_agents::ExecutionPolicy`):
//! destructive patterns are refused even with `--yes`.

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::display::{Display, Event};
use cluaiz_agents::{
    CancelHandle, ExecutionPolicy, PolicyDecision, SandboxTerminalRunner, SecurityMode,
};
use cluaiz_terminal::ApprovalResponse;
use serde_json::{json, Value};

/// Most characters of one tool result the model gets (a small context fills fast)
const MAX_RESULT: usize = 16_000;
const READ_LINES: usize = 400;
const SEARCH_HITS: usize = 100;
const LIST_ENTRIES: usize = 300;
/// Folders a search never walks into
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    ".next",
];

pub struct Tools {
    root: PathBuf,
    /// `--yes`: no questions
    pub allow_all: bool,
    /// Strict approval mode asks for workspace reads as well as changes and commands.
    pub strict: bool,
    /// What runs without asking: ask, accept-edits, plan (read only) or full (`--mode`, `/mode`)
    pub mode: Mode,
    /// Tools the user allowed for the rest of the session ("always")
    always: HashSet<String>,
    display: Display,
    pub cancelled: bool,
    /// Enabled skills, plugins and MCP servers from `~/.cluaiz/tools`, offered next to the built-ins
    installed: Vec<String>,
    installed_schemas: Vec<Value>,
    /// Files this request and the last one changed (for the summary, /undo and /diff)
    pub checkpoints: super::checkpoints::Checkpoints,
    /// `--allowed-tools` / `--disallowed-tools`
    pub rules: super::rules::Rules,
    /// A call was refused because nobody allowed it (headless exit code 2)
    pub refused: bool,
}

/// Most characters of installed tool schemas sent with every request (a small context fills fast)
const INSTALLED_SCHEMA_CHARS: usize = 12_000;
const BUILT_IN: &[&str] = &["read_file", "list_dir", "search", "write_file", "edit_file", "run_command"];

impl Tools {
    pub fn new(root: PathBuf, allow_all: bool) -> Self {
        let root = root.canonicalize().unwrap_or(root);
        Self {
            root,
            allow_all,
            strict: false,
            mode: if allow_all { Mode::Full } else { Mode::Ask },
            always: HashSet::new(),
            display: Display::default(),
            cancelled: false,
            installed: Vec::new(),
            installed_schemas: Vec::new(),
            checkpoints: Default::default(),
            rules: Default::default(),
            refused: false,
        }
    }

    /// Offers the enabled installed tools (the same registry and schemas the API uses) and returns
    /// the instructions of installed skills for the system prompt. Tools run in the working folder.
    pub async fn load_installed(&mut self) -> String {
        cluaiz_agents::ToolsEngine::set_workspace_dir(self.root.clone());
        let ids: Vec<String> = cluaiz_agents::ToolsEngine::list_all_tools()
            .unwrap_or_default()
            .into_iter()
            .filter(|tool| tool.enabled)
            .map(|tool| tool.id)
            .collect();
        if ids.is_empty() {
            return String::new();
        }
        let compiled = cluaiz_agents::ToolsEngine::compile_prompt_tools(&ids).await;
        let (schemas, left_out) = compiled.schemas_within(INSTALLED_SCHEMA_CHARS, "");
        if !left_out.is_empty() {
            self.display.emit(Event::Message(
                "Tools".into(),
                format!("{} installed tool(s) left out to save context: {}", left_out.len(), left_out.join(", ")),
            ));
        }
        self.installed_schemas = schemas
            .into_iter()
            .filter(|schema| {
                schema["function"]["name"].as_str().is_some_and(|name| !BUILT_IN.contains(&name))
            })
            .collect();
        self.installed = compiled.active_tool_ids;
        compiled.skill_instructions
    }

    /// Names of the installed tools offered to the model
    pub fn installed_names(&self) -> Vec<String> {
        self.installed_schemas
            .iter()
            .filter_map(|schema| schema["function"]["name"].as_str().map(str::to_owned))
            .collect()
    }

    /// Built-in schemas, then the installed ones
    pub fn all_schemas(&self) -> Value {
        let mut all = Self::schemas().as_array().cloned().unwrap_or_default();
        all.extend(self.installed_schemas.iter().cloned());
        Value::Array(all)
    }

    /// One call of an installed skill, plugin or MCP tool, asked first like a command
    async fn run_installed(&mut self, name: &str, arguments: &str) -> Result<(String, String), String> {
        let target = cluaiz_agents::ToolsEngine::resolve_function_target(name, &self.installed)
            .ok_or_else(|| {
                format!(
                    "there is no tool named '{name}'. Tools: {}",
                    BUILT_IN.iter().map(|n| n.to_string()).chain(self.installed_names()).collect::<Vec<_>>().join(", ")
                )
            })?;
        let what = format!("{} {}", target.category, target.component_id);
        self.ask(&format!("{}:{}", target.category, target.component_id), &what)?;
        let output = cluaiz_agents::ToolsEngine::execute_tool_by_name(
            &target.category,
            &target.component_id,
            target.sub_function.as_deref(),
            arguments,
        )
        .await
        .map_err(|error| format!("{error:#}"))?;
        Ok((format!("{what}: {} characters", output.len()), output))
    }

    pub fn set_display(&mut self, display: Display) {
        self.display = display;
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A human attachment uses the same canonical path and read authorization as file tools.
    pub fn attachment_path(&mut self, source: &str) -> Result<PathBuf, String> {
        let path = cluaiz_api::cep::roots::resolve(source, &self.root, false)
            .map_err(|e| format!("{e:?}"))?;
        self.authorize_read("attachment", &path)?;
        Ok(path)
    }

    pub fn require_approval(&mut self) {
        self.allow_all = false;
        self.strict = false;
        self.mode = Mode::Ask;
        self.always.clear();
    }

    /// `--mode` / `/mode`: full also allows every call, the others ask again from now on
    pub fn set_mode(&mut self, mode: Mode) {
        self.require_approval();
        self.mode = mode;
        self.allow_all = mode == Mode::Full;
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
        self.cancelled = false;
        if self.display.closing() || self.display.interrupted() {
            self.cancelled = true;
            return "Error: cancelled before tool execution".into();
        }
        let args: Value = match serde_json::from_str(arguments) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                let error = "Tool arguments are not a complete JSON object; nothing executed";
                self.display
                    .emit(Event::ToolStart(name.into(), String::new()));
                self.display.emit(Event::ToolEnd {
                    ok: false,
                    summary: error.into(),
                    content: String::new(),
                });
                return format!("Error: {error}");
            }
        };
        let target = args
            .get("path")
            .or_else(|| args.get("command"))
            .or_else(|| args.get("pattern"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.display
            .emit(Event::ToolStart(name.into(), target.into()));
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
            other => self.run_installed(other, arguments).await,
        };
        match result {
            Ok((summary, output)) => {
                self.display.emit(Event::ToolEnd {
                    ok: true,
                    summary,
                    content: output.clone(),
                });
                clip(&output)
            }
            Err(e) => {
                self.display.emit(Event::ToolEnd {
                    ok: false,
                    summary: e.clone(),
                    content: String::new(),
                });
                format!("Error: {e}")
            }
        }
    }

    fn read_file(
        &mut self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<(String, String), String> {
        let file = self.resolve(path)?;
        self.authorize_read("read_file", &file)?;
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Err("a binary file, not text".into());
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let first = offset.unwrap_or(1).max(1);
        let limit = limit.unwrap_or(READ_LINES).max(1);
        let shown: Vec<String> = lines
            .iter()
            .enumerate()
            .skip(first - 1)
            .take(limit)
            .map(|(i, l)| format!("{:>5}\t{}", i + 1, l))
            .collect();
        let mut out = shown.join("\n");
        let last = first - 1 + shown.len();
        if last < lines.len() {
            out.push_str(&format!(
                "\n({} more lines: read with offset {})",
                lines.len() - last,
                last + 1
            ));
        }
        if lines.is_empty() {
            out = "(empty file)".into();
        }
        Ok((format!("{} lines", shown.len()), out))
    }

    fn list_dir(&mut self, path: &str) -> Result<(String, String), String> {
        let dir = self.resolve(path)?;
        self.authorize_read("list_dir", &dir)?;
        let entries = directory_entries(&dir).map_err(|error| error.to_string())?;
        let total = entries.len();
        let mut out: Vec<String> = entries
            .into_iter()
            .take(LIST_ENTRIES)
            .map(|(d, n)| if d { format!("{n}/") } else { n })
            .collect();
        if total > LIST_ENTRIES {
            out.push("(more entries; narrow the folder)".into());
        }
        Ok((
            format!(
                "{}{} entries",
                total.min(LIST_ENTRIES),
                if total > LIST_ENTRIES { "+" } else { "" }
            ),
            if out.is_empty() {
                "(empty folder)".into()
            } else {
                out.join("\n")
            },
        ))
    }

    fn search(
        &mut self,
        pattern: &str,
        path: &str,
        glob: Option<String>,
    ) -> Result<(String, String), String> {
        let start = self.resolve(path)?;
        self.authorize_read("search", &start)?;
        let re = regex::Regex::new(pattern)
            .or_else(|_| regex::Regex::new(&regex::escape(pattern)))
            .map_err(|e| e.to_string())?;
        let name_matches = |name: &str| match glob.as_deref() {
            Some(g) => match g.strip_prefix('*') {
                Some(suffix) => name.ends_with(suffix),
                None => name == g,
            },
            None => true,
        };
        let mut hits = Vec::new();
        let mut more = 0usize;
        let walker = walkdir::WalkDir::new(&start).into_iter().filter_entry(|e| {
            e.depth() == 0
                || !(e.file_type().is_dir()
                    && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() || !name_matches(&entry.file_name().to_string_lossy()) {
                continue;
            }
            if entry
                .metadata()
                .map(|m| m.len() > 2_000_000)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
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
        Ok((
            format!("{count} matches"),
            if hits.is_empty() {
                "No matches.".into()
            } else {
                hits.join("\n")
            },
        ))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(String, String), String> {
        let file = self.resolve(path)?;
        let exists = file.exists();
        let lines = content.lines().count();
        // What it replaces: the diff shows removed lines too, not the whole file as added
        let old = if exists { std::fs::read_to_string(&file).unwrap_or_default() } else { String::new() };
        let diff = super::line_diff::lines(&old, content);
        self.display.emit(Event::Diff(diff.clone()));
        self.ask(
            "write_file",
            &format!(
                "{} {}\n{diff}",
                if exists { "replace" } else { "create" },
                cluaiz_api::cep::roots::display(&file)
            ),
        )?;
        self.check_running()?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        self.checkpoints.before_write(&file);
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        let what = if exists { "replaced" } else { "created" };
        Ok((
            format!("{what}, {lines} lines"),
            format!("{} {what} ({lines} lines).", self.show(&file)),
        ))
    }

    fn edit_file(
        &mut self,
        path: &str,
        old: &str,
        new: &str,
        replace_all: bool,
    ) -> Result<(String, String), String> {
        let file = self.resolve(path)?;
        self.allow_outside("edit_file", &file)?;
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        // A file with Windows line breaks matches text the model wrote with plain ones
        let (old, new) = if !text.contains(old) && text.contains("\r\n") {
            (
                old.replace("\r\n", "\n").replace('\n', "\r\n"),
                new.replace("\r\n", "\n").replace('\n', "\r\n"),
            )
        } else {
            (old.to_string(), new.to_string())
        };
        if old.is_empty() {
            return Err("old_text is empty".into());
        }
        let found = text.matches(&old).count();
        if found == 0 {
            return Err(
                "old_text is not in the file; read the file and copy the text exactly".into(),
            );
        }
        if found > 1 && !replace_all {
            return Err(format!("old_text is in the file {found} times; add the lines around it, or set replace_all"));
        }
        let diff = super::line_diff::lines(&old, &new);
        self.display.emit(Event::Diff(diff.clone()));
        self.ask(
            "edit_file",
            &format!("edit {}\n{diff}", cluaiz_api::cep::roots::display(&file)),
        )?;
        self.check_running()?;
        if std::fs::read_to_string(&file).map_err(|error| error.to_string())? != text {
            return Err(
                "File changed while awaiting approval; read it again before editing".into(),
            );
        }
        let updated = if replace_all {
            text.replace(&old, &new)
        } else {
            text.replacen(&old, &new, 1)
        };
        self.checkpoints.before_write(&file);
        std::fs::write(&file, updated).map_err(|e| format!("{}: {e}", self.show(&file)))?;
        let n = if replace_all { found } else { 1 };
        Ok((
            format!("{n} replaced"),
            format!("{} edited ({n} replaced).", self.show(&file)),
        ))
    }

    async fn run_command(
        &mut self,
        command: &str,
        timeout: usize,
    ) -> Result<(String, String), String> {
        let mode =
            match ExecutionPolicy::evaluate(command, &self.root, SecurityMode::RequireApproval) {
                PolicyDecision::Deny { reason } => return Err(format!("refused: {reason}")),
                PolicyDecision::Allow => {
                    if self.strict {
                        self.ask("run_command", command)?;
                    }
                    SecurityMode::RequireApproval
                }
                PolicyDecision::AskConfirmation { .. } => {
                    self.ask(
                        "run_command",
                        &format!(
                            "{}\nin {}",
                            command,
                            cluaiz_api::cep::roots::display(&self.root)
                        ),
                    )?;
                    SecurityMode::AlwaysAllow
                }
            };
        self.execute(command, mode, timeout).await
    }

    /// `!<command>` typed by the user: runs without asking (they typed it) and returns the exit and
    /// output, which the conversation keeps so the model knows what happened
    pub async fn run_user_command(&mut self, command: &str) -> (bool, String) {
        match self.execute(command, SecurityMode::AlwaysAllow, 600).await {
            Ok((_, out)) => (true, out),
            Err(out) => (false, out),
        }
    }

    async fn execute(&mut self, command: &str, mode: SecurityMode, timeout: usize) -> Result<(String, String), String> {
        // Ctrl+C stops the command (the handler sets the cancel signal while this runs)
        let (cancel, cancel_rx) = CancelHandle::new();
        self.check_running()?;
        crate::chat::GENERATING.store(true, Ordering::SeqCst);
        let display = self.display.clone();
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if display.closing()
                    || display.interrupted()
                    || cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst)
                {
                    cancel.cancel();
                    break;
                }
            }
        });
        let result = SandboxTerminalRunner::execute_advanced(
            command,
            &self.root,
            mode,
            None,
            None,
            Some(timeout as u64),
            Some(cancel_rx),
        )
        .await;
        watcher.abort();
        crate::chat::GENERATING.store(false, Ordering::SeqCst);
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
        let run = result.map_err(|e| e.to_string())?;
        self.cancelled = run.cancelled;
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
        let state = if run.cancelled {
            "stopped".to_string()
        } else {
            format!("exit {}", run.exit_code)
        };
        if run.cancelled || run.exit_code != 0 {
            return Err(out);
        }
        Ok((format!("{state}, {lines} lines of output"), out))
    }

    /// Shared canonical resolver also handles not-yet-created output paths.
    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        cluaiz_api::cep::roots::resolve(path, &self.root, true).map_err(|error| error.to_string())
    }

    /// The path as shown: relative inside the working folder
    fn show(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rel) if rel.as_os_str().is_empty() => ".".into(),
            Ok(rel) => rel.display().to_string().replace('\\', "/"),
            Err(_) => cluaiz_api::cep::roots::display(path),
        }
    }

    fn allow_outside(&mut self, tool: &str, path: &Path) -> Result<(), String> {
        if path.starts_with(&self.root) {
            return Ok(());
        }
        self.ask(
            tool,
            &format!(
                "outside the working folder: {}",
                cluaiz_api::cep::roots::display(path)
            ),
        )
    }

    fn authorize_read(&mut self, tool: &str, path: &Path) -> Result<(), String> {
        if self.strict {
            self.ask(tool, &cluaiz_api::cep::roots::display(path))
        } else {
            self.allow_outside(tool, path)
        }
    }

    fn check_running(&mut self) -> Result<(), String> {
        if self.display.closing()
            || self.display.interrupted()
            || cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst)
        {
            self.cancelled = true;
            Err("Cancelled before execution; nothing changed".into())
        } else {
            Ok(())
        }
    }

    /// The user's answer to one call; Err is what the model is told when it is not allowed.
    fn ask(&mut self, tool: &str, why: &str) -> Result<(), String> {
        match self.rules.check(tool, why) {
            super::rules::Verdict::Deny(reason) => {
                self.refused = true;
                return Err(reason);
            }
            super::rules::Verdict::Allow => return Ok(()),
            super::rules::Verdict::Ask => {}
        }
        let edit = matches!(tool, "write_file" | "edit_file");
        match self.mode {
            Mode::Plan if edit || tool == "run_command" || tool.contains(':') => {
                return Err("plan mode: nothing is changed or run now. Read what you need, then write the plan (files, changes, commands) for the user to approve".into());
            }
            Mode::AcceptEdits if edit && !self.strict => return Ok(()),
            _ => {}
        }
        if self.allow_all || self.always.contains(tool) {
            return Ok(());
        }
        if !self.display.screen_enabled() && !std::io::stdin().is_terminal() {
            self.refused = true;
            // Told to the model: what to do now, not which flags the user could have passed
            return Err("not allowed: nobody can approve this call in this run; finish the task without it, or say what you would need".into());
        }
        let answer = tokio::task::block_in_place(|| self.display.approve(tool, why));
        match answer {
            ApprovalResponse::Allow => Ok(()),
            ApprovalResponse::Session => {
                self.always.insert(tool.to_string());
                Ok(())
            }
            ApprovalResponse::Feedback(text) => {
                Err(format!("The user did not allow this and said: {text}"))
            }
            ApprovalResponse::Deny => Err("the user did not allow this; nothing executed".into()),
        }
    }
}


/// Bounded directory enumeration shared by the tool formatter and passive completion.
pub(super) fn directory_entries(dir: &Path) -> std::io::Result<Vec<(bool, String)>> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .take(LIST_ENTRIES + 1)
        .map(|entry| {
            (
                entry.file_type().is_ok_and(|kind| kind.is_dir()),
                entry.file_name().to_string_lossy().into_owned(),
            )
        })
        .collect();
    entries.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    Ok(entries)
}

/// A long result keeps its start and end
pub(crate) fn clip(text: &str) -> String {
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
    format!(
        "{}\n… ({} characters left out) …\n{}",
        &text[..head],
        tail - head,
        &text[tail..]
    )
}

#[cfg(test)]
#[path = "../../tests/agent/tools.rs"]
mod tests;

/// What runs without asking. Reads in the working folder and read-only commands always run;
/// destructive commands and `--disallowed-tools` are always refused.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Every change and command asks
    #[default]
    Ask,
    /// File writes and edits run; commands ask
    AcceptEdits,
    /// Read only: changes and commands are refused, the model writes a plan instead
    Plan,
    /// Everything runs without asking
    Full,
}

impl Mode {
    /// The App's security mode (`agent_security_mode` in permission.json) as an agent mode:
    /// full_access = Full, sandboxed = Ask; strict is Ask plus `strict`
    pub fn from_global(value: &str) -> (Self, bool) {
        match value {
            "full_access" => (Mode::Full, false),
            "strict" => (Mode::Ask, true),
            _ => (Mode::Ask, false),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Workspace-Gated",
            Mode::AcceptEdits => "Accept edits (this session)",
            Mode::Plan => "Plan, read only (this session)",
            Mode::Full => "Full Access",
        }
    }
}
