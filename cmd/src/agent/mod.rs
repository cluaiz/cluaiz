//! `cluaiz agent`: a coding agent in the terminal. The model works in a folder with tools (read,
//! search, edit files, run commands): each answer that calls tools gets their results and goes on,
//! until it answers without a call. Tools are offered the way an API request's `tools` are (the
//! model's own template renders them, `<tool_call>` blocks come back), on the same loaded model and
//! settings as `cluaiz run`.

mod attachments;
mod commands;
mod completion;
mod display;
mod terminal;
mod terminal_state;
mod checkpoints;
mod rules;
pub mod sessions;
pub mod tools;
mod validation;

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use cluaiz_core::media::PreparedPart;
use cluaiz_core::ChatMessage;
use cluaiz_runtime::EngineHost;
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;

use crate::chat::{print, ChatOptions, Session};
use crate::cli::args::{LoadArgs, SampleArgs, ThinkArgs};
use crate::cli::models::{self, Lookup};
use display::{Display, Event};
use tools::Tools;

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum UiMode {
    #[default]
    Auto,
    Plain,
    Tui,
}

/// Calls one request may make before the agent stops and asks the user
const MAX_STEPS: usize = 40;
/// Most characters of AGENTS.md put in the system prompt
const MAX_PROJECT_NOTES: usize = 8_000;

#[derive(clap::Args, Default)]
pub struct AgentArgs {
    /// Terminal interface: auto for interactive terminals, plain for scripts
    #[arg(long, value_enum, default_value = "auto")]
    pub ui: UiMode,
    /// Task to do, then exit (no task: chat; piped stdin is the task)
    #[arg(trailing_var_arg = true)]
    pub words: Vec<String>,
    /// Task to do, then exit
    #[arg(
        short = 'p',
        long = "prompt",
        value_name = "TEXT",
        conflicts_with = "words"
    )]
    pub prompt: Option<String>,
    /// Installed model id or path to a .gguf file (default: the active chat model)
    #[arg(short = 'm', long = "model", value_name = "MODEL")]
    pub model: Option<String>,
    /// Folder to work in (default: the current folder)
    #[arg(short = 'C', long = "dir", value_name = "PATH")]
    pub dir: Option<PathBuf>,
    /// Allow every file change and command without asking (destructive commands stay refused);
    /// the same as --mode full
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// What runs without asking: ask (default), accept-edits (file changes run, commands ask),
    /// plan (read only: the model writes a plan) or full. Default: the folder's .cluaiz/agent.json
    #[arg(long, value_enum)]
    pub mode: Option<tools::Mode>,
    /// Keep chatting after doing the task
    #[arg(short = 'i', long)]
    pub interactive: bool,
    /// Load the model in this process even when `cluaiz serve` is running (default: use the server)
    #[arg(long)]
    pub local: bool,
    /// Go on with the last conversation in this folder
    #[arg(long = "continue", conflicts_with = "resume")]
    pub continue_last: bool,
    /// Go on with a saved conversation of this folder (its id: see /sessions)
    #[arg(long, value_name = "ID")]
    pub resume: Option<String>,
    /// Output of a task: text, json (one result object at the end) or stream-json (one JSON
    /// event per line, then the result)
    #[arg(long = "output-format", value_enum, default_value_t = OutputFormat::Text)]
    pub output_format: OutputFormat,
    /// Calls that run without asking, e.g. "read_file,run_command(git *),edit_file(src/*)"
    #[arg(long = "allowed-tools", value_name = "RULES")]
    pub allowed_tools: Vec<String>,
    /// Calls that are always refused, e.g. "run_command(git push*)"
    #[arg(long = "disallowed-tools", value_name = "RULES")]
    pub disallowed_tools: Vec<String>,
    /// Most tool calls for one request (default 40)
    #[arg(long = "max-steps", value_name = "N")]
    pub max_steps: Option<usize>,
    /// Added to the agent's system prompt (-sys)
    #[arg(long = "system-prompt", value_name = "TEXT")]
    pub system_prompt: Option<String>,
    /// Most tokens per answer (default: what the context leaves)
    #[arg(
        short = 'n',
        long = "n-predict",
        visible_alias = "predict",
        value_name = "N"
    )]
    pub n_predict: Option<usize>,
    #[command(flatten)]
    pub load: LoadArgs,
    #[command(flatten)]
    pub sample: SampleArgs,
    #[command(flatten)]
    pub think: ThinkArgs,
}

impl AgentArgs {
    pub fn screen_enabled(&self) -> bool {
        !matches!(self.ui, UiMode::Plain)
            && (self.interactive
                || self.prompt.is_none() && self.words.is_empty()
                || matches!(self.ui, UiMode::Tui))
            && cluaiz_terminal::terminal::is_interactive()
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    StreamJson,
}

pub async fn execute(args: AgentArgs, verbose: bool) -> Result<()> {
    let headless = args.output_format != OutputFormat::Text;
    let screen_enabled = args.screen_enabled() && !headless;
    if matches!(args.ui, UiMode::Tui) && !screen_enabled {
        return Err(eyre!(
            "Interactive terminal required; use --ui plain for redirected output"
        ));
    }
    let name = match args.model.clone() {
        Some(name) => name,
        None => models::chat_model_or_choose()?,
    };
    let (path, id) = match models::lookup(&name) {
        Lookup::Found(path, id) => (path, id),
        _ => models::resolve_installed(&name)?,
    };
    let dir = match &args.dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir()?,
    };
    let dir = dunce(
        dir.canonicalize()
            .map_err(|e| eyre!("{}: {}", dir.display(), e))?,
    );

    let mut task = match (&args.prompt, args.words.is_empty()) {
        (Some(p), _) => Some(p.clone()),
        (None, false) => Some(args.words.join(" ")),
        _ => None,
    };
    if task.is_none() && !args.interactive && !io::stdin().is_terminal() {
        let mut piped = String::new();
        io::stdin().read_to_string(&mut piped)?;
        task = Some(piped.trim().to_string()).filter(|t| !t.is_empty());
    }
    let interactive = args.interactive || task.is_none();
    if headless && (interactive || task.is_none()) {
        return Err(eyre!("--output-format json / stream-json runs one task: give it (cluaiz agent -p \"...\" --output-format json)"));
    }

    let mut options = ChatOptions::new(&args.load, &args.sample, &args.think);
    options.max_tokens = args.n_predict;
    options.verbose = verbose;
    options.local = args.local;
    if !screen_enabled {
        crate::chat::install_interrupt_handler();
    }
    let session = Session::open(Arc::new(EngineHost::new()), path, id, options).await?;
    if (interactive || verbose) && !screen_enabled {
        print::plan(&session).await;
    }

    let mut agent = Agent {
        system: system_prompt(&dir, args.system_prompt.as_deref()),
        tools: Tools::new(dir, args.yes),
        session,
        history: Vec::new(),
        display: Display::default(),
        session_id: sessions::new_id(),
        usage: None,
        compact_next: false,
        max_steps: args.max_steps.unwrap_or(MAX_STEPS).max(1),
    };
    // The folder's own permissions (.cluaiz/agent.json), then this run's flags on top
    let project = rules::Project::load(agent.tools.root());
    let allowed: Vec<String> = project.allow.iter().cloned().chain(args.allowed_tools.iter().cloned()).collect();
    let denied: Vec<String> = project.deny.iter().cloned().chain(args.disallowed_tools.iter().cloned()).collect();
    agent.tools.rules = rules::Rules::new(&allowed, &denied, agent.tools.root());
    // The mode: --yes / --mode, else the folder's agent.json, else the App's global security mode
    match if args.yes { Some(tools::Mode::Full) } else { args.mode.or(project.mode) } {
        Some(mode) => agent.tools.set_mode(mode),
        None => {
            let (mode, strict) = tools::Mode::from_global(&cluaiz_security::PermissionSchema::load().agent_security_mode);
            agent.tools.set_mode(mode);
            agent.tools.strict = strict;
        }
    }
    if headless {
        let display = Display::json(args.output_format == OutputFormat::StreamJson);
        agent.tools.set_display(display.clone());
        agent.display = display;
    }
    if args.continue_last || args.resume.is_some() {
        let saved = sessions::load(agent.tools.root(), args.resume.as_deref())
            .ok_or_else(|| eyre!("no saved conversation {}in this folder (see /sessions)", args.resume.as_deref().map(|id| format!("{id} ")).unwrap_or_default()))?;
        agent.history = sessions::history(&saved);
        agent.session_id = saved.id;
    }
    // Installed skills, plugins and MCP servers, with the skills' instructions
    let skills = agent.tools.load_installed().await;
    if !skills.trim().is_empty() {
        agent.system.push_str(&format!("\n\n{}", skills.trim()));
    }
    if screen_enabled {
        return terminal::run(agent, task, interactive).await;
    }
    if interactive {
        eprintln!("{}", context_note(agent.session.plan.context_window).dimmed());
        eprintln!("{} {}", "Working in".dimmed(), agent.tools.root().display());
        eprintln!("{}", "Type a task, /? for help, /bye to exit.".dimmed());
    }
    if headless {
        let task = task.unwrap_or_default();
        let outcome = agent.turn(&task).await;
        let code = agent.finish_headless(outcome, args.output_format);
        std::process::exit(code);
    }
    if let Some(task) = task {
        if interactive {
            println!("{} {}", ">>>".bold(), task);
        }
        agent.turn(&task).await?;
        if !interactive {
            return Ok(());
        }
        println!();
    }
    loop {
        let Some(input) = crate::chat::repl::read_message() else {
            println!();
            return Ok(());
        };
        let text = input.trim();
        if text.is_empty() {
            continue;
        }
        match agent.command(text) {
            Some(true) => return Ok(()),
            Some(false) => {}
            None => {
                if let Err(e) = agent.turn(text).await {
                    eprintln!("{} {}", "error:".red(), e);
                }
                println!();
            }
        }
    }
}

struct Agent {
    session: Session,
    tools: Tools,
    system: String,
    history: Vec<ChatMessage<Vec<PreparedPart>>>,
    display: Display,
    /// The saved conversation this one writes to (`~/.cluaiz/sessions`)
    session_id: String,
    /// How full the context was after the last answer (tokens, context size)
    usage: Option<(usize, usize)>,
    /// Most tool calls for one request
    max_steps: usize,
    /// The Engine left out old messages to fit: compact before the next request
    compact_next: bool,
}

impl Agent {
    fn status(&self) -> String {
        format!(
            "{} · {} · Think {}",
            context_note(self.session.plan.context_window),
            if self.tools.strict {
                "Strict Approval"
            } else {
                self.tools.mode.label()
            },
            self.session
                .options
                .think
                .unwrap_or(cluaiz_core::Settings::current().optimization.think_mode)
                .name()
        )
    }

    /// Commands use the same implementation in both terminal modes.
    fn command(&mut self, text: &str) -> Option<bool> {
        let message = match text {
            "/bye" | "/exit" => return Some(true),
            "/?" | "/help" => commands::help(),
            "/theme" => {
                self.display.emit(Event::Themes);
                return Some(false);
            }
            "/clear" => {
                self.history.clear();
                // The cleared conversation stays saved; the next request starts a new one
                self.session_id = sessions::new_id();
                "Conversation cleared.".into()
            }
            "/sessions" => {
                let saved = sessions::list(self.tools.root());
                if saved.is_empty() {
                    "No saved conversations in this folder yet.".into()
                } else {
                    let rows: Vec<String> = saved
                        .iter()
                        .take(15)
                        .map(|s| format!("{}  {}  {:>3} messages  {}{}", s.id, s.updated, s.messages.len(), s.title, if s.id == self.session_id { "  (this one)" } else { "" }))
                        .collect();
                    format!("{}\nGo on with one: cluaiz agent --resume <id>   (the last one: --continue)", rows.join("\n"))
                }
            }
            "/compact" => return None,
            _ if text.starts_with("/model ") => return None,
            _ if text.starts_with("/effort") => {
                let level = text.trim_start_matches("/effort").trim();
                match cluaiz_core::ThinkingBudget::parse(level) {
                    Ok(effort) => {
                        self.session.options.effort = Some(effort);
                        format!("Thinking effort: {level}.")
                    }
                    Err(_) => "Choose: /effort auto | low | medium | high | max (or a number of tokens)".into(),
                }
            }
            _ if text.starts_with("/mode") => {
                use clap::ValueEnum;
                let value = text.trim_start_matches("/mode").trim();
                // The App's three modes: saved globally, so the App and every agent use them too
                if matches!(value, "full_access" | "sandboxed" | "strict") {
                    let mut permissions = cluaiz_security::PermissionSchema::load();
                    permissions.agent_security_mode = value.to_owned();
                    let saved = permissions.save();
                    let (mode, strict) = tools::Mode::from_global(value);
                    self.tools.set_mode(mode);
                    self.tools.strict = strict;
                    self.display.emit(Event::Ready(self.status()));
                    return match saved {
                        Ok(()) => { self.display.emit(Event::Message("Agent".into(), format!("Security mode: {} (saved for the App and every agent).", self.status_mode()))); Some(false) }
                        Err(error) => { self.display.emit(Event::Message("Agent".into(), format!("Mode set for this session; not saved: {error}"))); Some(false) }
                    };
                }
                match tools::Mode::from_str(value, true) {
                    Ok(mode) => {
                        self.tools.set_mode(mode);
                        self.display.emit(Event::Ready(self.status()));
                        format!("Mode: {}.", mode.label())
                    }
                    Err(_) => format!("Mode now: {}. Choose: /mode full_access | sandboxed | strict (saved), or plan | accept-edits (this session)", self.status_mode()),
                }
            }
            "/undo" => {
                let root = self.tools.root().to_path_buf();
                let show = |path: &std::path::Path| path.strip_prefix(&root).unwrap_or(path).display().to_string().replace('\\', "/");
                match self.tools.checkpoints.undo(show) {
                    Ok(done) => {
                        // The model is told, so it does not build on the undone change
                        self.history.push(message("user", format!("(The user undid your last file changes. {done}. Read the files again before changing them.)")));
                        self.history.push(message("assistant", "Understood.".into()));
                        done
                    }
                    Err(nothing) => nothing,
                }
            }
            "/diff" => diff(self.tools.root(), &self.tools.checkpoints.last_files()),
            "/yes" => {
                self.tools.set_mode(tools::Mode::Full);
                "Changes and commands now run without asking (existing hard denials still apply)."
                    .into()
            }
            "/ask" => {
                self.tools.require_approval();
                "Each change and command asks first.".into()
            }
            "/strict" => {
                self.tools.require_approval();
                self.tools.strict = true;
                "Strict approvals: every built-in tool asks, including workspace reads. This is an approval policy, not OS process isolation.".into()
            }
            "/permissions" => "Ask: workspace reads run; changes/commands ask.\nStrict: every built-in tool asks.\n/ask · /strict · /yes (session grants).\nThese policies do not provide an OS sandbox.".into(),
            "/think" | "/think on" | "/think off" | "/think auto" => {
                use cluaiz_core::FeatureState;
                let current = self.session.options.think.unwrap_or(cluaiz_core::Settings::current().optimization.think_mode);
                let mode = match text { "/think on" => FeatureState::On, "/think off" => FeatureState::Off,
                    "/think auto" => FeatureState::Auto, _ if current == FeatureState::Off => FeatureState::On, _ => FeatureState::Off };
                self.session.options.think = Some(mode);
                format!("Thinking {} for the next request (subject to the selected model's capabilities).", mode.name())
            }
            _ if text.starts_with('/') => format!("Unknown command {text}; use /help"),
            _ => return None,
        };
        self.display.emit(Event::Message("Agent".into(), message));
        Some(false)
    }

    /// One request: answers and tool calls until an answer has no call.
    async fn turn(&mut self, text: &str) -> Result<()> {
        self.display.begin_turn();
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, std::sync::atomic::Ordering::SeqCst);
        self.turn_prepared(text, Vec::new()).await
    }

    /// One request, then the conversation is saved (also after an error: what happened stays)
    async fn turn_prepared(&mut self, text: &str, parts: Vec<PreparedPart>) -> Result<()> {
        let result = if text.trim() == "/compact" {
            self.compact().await
        } else if let Some(id) = text.trim().strip_prefix("/model ") {
            self.switch_model(id.trim()).await
        } else if let Some(command) = text.trim().strip_prefix('!') {
            self.user_command(command.trim()).await;
            Ok(())
        } else {
            self.request(text, parts).await
        };
        // What the request changed, one line; /undo puts it back
        let root = self.tools.root().to_path_buf();
        let show = |path: &std::path::Path| path.strip_prefix(&root).unwrap_or(path).display().to_string().replace('\\', "/");
        if let Some(changed) = self.tools.checkpoints.finish(show) {
            self.display.emit(Event::Message("Files".into(), format!("{changed}   (/undo puts them back, /diff shows them)")));
        }
        if !self.history.is_empty() {
            if let Err(error) = sessions::save(self.tools.root(), &self.session_id, &self.session.model_id, &self.history) {
                self.display.emit(Event::Message("Session".into(), format!("Not saved: {error}")));
            }
        }
        result
    }

    /// The headless result (json: one object with every event; stream-json: the last line) and the
    /// exit code: 0 done, 1 error, 2 a call was refused, 130 cancelled
    fn finish_headless(&mut self, outcome: Result<()>, format: OutputFormat) -> i32 {
        let answer = self
            .history
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .map(|m| m.content.iter().filter_map(|part| match part {
                PreparedPart::Text { text } => Some(cluaiz_core::tool_calls::split_calls(text).0),
                _ => None,
            }).collect::<String>())
            .unwrap_or_default();
        let root = self.tools.root().to_path_buf();
        let files: Vec<String> = self.tools.checkpoints.last_files().iter()
            .map(|path| path.strip_prefix(&root).unwrap_or(path).display().to_string().replace('\\', "/"))
            .collect();
        let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(std::sync::atomic::Ordering::SeqCst) || self.display.interrupted();
        let (code, error) = match &outcome {
            Err(error) => (1, Some(format!("{error:#}"))),
            Ok(()) if cancelled => (130, None),
            Ok(()) if self.tools.refused => (2, None),
            Ok(()) => (0, None),
        };
        let mut result = serde_json::json!({
            "type": "result",
            "result": answer.trim(),
            "is_error": code == 1,
            "error": error,
            "refused_calls": self.tools.refused,
            "files_changed": files,
            "session_id": self.session_id,
            "model": self.session.model_id,
            "exit_code": code,
        });
        if format == OutputFormat::Json {
            result["events"] = serde_json::Value::Array(self.display.json_events());
        }
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{result}");
        let _ = out.flush();
        code
    }

    /// Remembers how full the context is and shows it in the footer
    fn note_usage(&mut self, report: &crate::chat::TurnReport) {
        if report.dropped > 0 {
            self.compact_next = true;
        }
        if let Some((used, size)) = report.context.filter(|(_, size)| *size > 0) {
            self.usage = Some((used, size));
            // As llama.cpp measured it: generation speed, and the time with the prompt read
            let speed = report.timings.as_ref().filter(|t| t.predicted_n > 0).map(|t| {
                format!("{:.1} tok/s · {:.1}s", t.predicted_per_second, (t.prompt_ms + t.predicted_ms) / 1000.0)
            });
            self.display.emit(Event::Context { used, size, speed });
        }
    }

    /// The mode as the App names it
    fn status_mode(&self) -> &'static str {
        if self.tools.strict { "Strict Approval" } else { self.tools.mode.label() }
    }

    /// `/model <id>`: the conversation goes on with another installed model
    async fn switch_model(&mut self, id: &str) -> Result<()> {
        let (path, id) = models::resolve_installed(id)?;
        self.display.emit(Event::Message("Model".into(), format!("Loading {id}…")));
        self.session.switch(path, id.clone()).await?;
        self.usage = None;
        self.display.emit(Event::Message("Model".into(), format!("Now using {id}.")));
        self.display.emit(Event::Ready(self.status()));
        Ok(())
    }

    /// `!<command>`: the user runs a command in the folder (like Claude Code's shell mode); the
    /// output is shown and kept in the conversation, so the next request can use it
    async fn user_command(&mut self, command: &str) {
        if command.is_empty() {
            self.display.emit(Event::Message("Shell".into(), "Type a command after !, for example: !git status".into()));
            return;
        }
        let (ok, out) = self.tools.run_user_command(command).await;
        const SHOWN: usize = 40;
        let lines: Vec<&str> = out.lines().collect();
        let mut shown = lines.iter().take(SHOWN).copied().collect::<Vec<_>>().join("\n");
        if lines.len() > SHOWN {
            shown.push_str(&format!("\n… {} more lines (the agent sees them all)", lines.len() - SHOWN));
        }
        self.display.emit(Event::Message(format!("! {command}{}", if ok { "" } else { "  (failed)" }), shown));
        self.history.push(message("user", format!("(I ran this command myself in the working folder.)\n$ {command}\n{}", tools::clip(&out))));
        self.history.push(message("assistant", "Noted.".into()));
    }

    /// `/compact`: the model writes a summary of the conversation, which then replaces it
    async fn compact(&mut self) -> Result<()> {
        if self.history.is_empty() {
            self.display.emit(Event::Message("Compact".into(), "Nothing to compact yet.".into()));
            return Ok(());
        }
        let before = self.history.len();
        // The conversation goes in as one text that fits: sent as messages, a history that no
        // longer fits loses its middle in the Engine and the model is left with nothing to summarize
        let size = self.usage.map_or(SMALL_CONTEXT as usize, |(_, size)| size);
        let messages = vec![
            message("system", "You summarize a coding agent's conversation for the agent itself.".into()),
            message("user", format!("{COMPACT_REQUEST}\n\n<conversation>\n{}\n</conversation>", compact_transcript(&self.history, size * 3 / 2))),
        ];
        let mut out = self.display.output(false);
        let (summary, report) = self.session.generate_prepared(messages, None, &mut *out).await?;
        if report.cancelled || summary.trim().is_empty() {
            return Err(eyre!("Compact stopped; the conversation is unchanged"));
        }
        self.history = vec![
            message("user", format!("Summary of our conversation so far (it replaced the full history):\n{}", summary.trim())),
            message("assistant", "Understood. I will continue from this summary.".into()),
        ];
        self.usage = None;
        self.compact_next = false;
        self.display.emit(Event::Usage("Context compacted".into()));
        self.display.emit(Event::Message("Compact".into(), format!("{before} messages replaced by this summary.")));
        Ok(())
    }

    async fn request(&mut self, text: &str, mut parts: Vec<PreparedPart>) -> Result<()> {
        if self.display.closing() {
            return Ok(());
        }
        // Like Claude Code: a nearly full context is compacted before the next request, so the
        // Engine does not have to drop the oldest messages unseen
        let full = self.usage.is_some_and(|(used, size)| size > 0 && used * 100 >= size * AUTO_COMPACT_PCT);
        if (full || self.compact_next) && self.history.len() >= 4 {
            let why = match self.usage {
                Some((used, size)) if full => format!("Context {}% full", used * 100 / size),
                _ => "The oldest messages no longer fit".to_owned(),
            };
            self.display.emit(Event::Message("Compact".into(), format!("{why}: summarizing the conversation before this request.")));
            if let Err(error) = self.compact().await {
                self.display.emit(Event::Message("Compact".into(), format!("{error}; going on without it.")));
            }
        }
        parts.insert(
            0,
            PreparedPart::Text {
                text: text.to_owned(),
            },
        );
        let user = ChatMessage {
            role: "user".into(),
            content: parts,
        };
        let retained = PreparedPart::request_bytes(&self.history)
            .map_err(|error| eyre!("{error:#}"))?
            .checked_add(
                PreparedPart::request_bytes(std::slice::from_ref(&user))
                    .map_err(|error| eyre!("{error:#}"))?,
            )
            .ok_or_else(|| eyre!("Chat attachment byte count overflow"))?;
        self.session
            .options
            .settings()?
            .media
            .limits
            .check_decoded_bytes(retained)?;
        self.history.push(user);
        let schemas = self.tools.all_schemas();
        for _ in 0..self.max_steps {
            if self.display.closing()
                || self.display.interrupted()
                || cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(eyre!("Request cancelled before generation"));
            }
            let (answer, report) = loop {
                let mut messages = Vec::with_capacity(self.history.len() + 1);
                messages.push(message("system", self.system.clone()));
                messages.extend(self.history.iter().cloned());
                let mut out = self.display.output(self.session.options.show_thinking);
                match self
                    .session
                    .generate_prepared(messages, Some(&schemas), &mut *out)
                    .await
                {
                    Ok(done) => break done,
                    Err(e)
                        if e.to_string().contains("exceeds the available context")
                            && self.make_room() =>
                    {
                        self.display.emit(Event::Message(
                            "Context".into(),
                            "Context full: the oldest part of the conversation was left out."
                                .into(),
                        ));
                    }
                    Err(e) => return Err(e),
                }
            };
            if self.session.options.verbose && !self.display.screen_enabled() {
                print::stats(&report);
            }
            self.note_usage(&report);
            if report.cancelled || self.display.interrupted() {
                let said = answer
                    .split(cluaiz_core::tool_calls::OPEN)
                    .next()
                    .unwrap_or_default();
                self.history.push(message("assistant", said.to_owned()));
                self.display.emit(Event::Message(
                    "Stopped".into(),
                    "Generation cancelled.".into(),
                ));
                return Ok(());
            }
            let (said, calls) =
                validation::calls(&answer, report.stop).map_err(|error| eyre!(error))?;
            if calls.is_empty() {
                self.history.push(message("assistant", said));
                return Ok(());
            }
            // Kept in the Engine's form, as the API keeps a conversation with calls
            let mut kept = said;
            for (name, arguments) in &calls {
                kept.push('\n');
                kept.push_str(&cluaiz_core::tool_calls::canonical_call(name, arguments));
            }
            self.history
                .push(message("assistant", kept.trim().to_string()));
            for (name, arguments) in &calls {
                if self.display.closing() || self.display.interrupted() {
                    return Ok(());
                }
                let result = self.tools.run(name, arguments).await;
                self.history.push(message("tool", result));
                if self.tools.cancelled {
                    return Ok(());
                }
            }
        }
        self.display.emit(Event::Message(
            "Stopped".into(),
            format!("Stopped after {} steps; say how to go on.", self.max_steps),
        ));
        Ok(())
    }

    /// Room in a full context: the oldest earlier request goes, else the oldest long tool result of
    /// this one is replaced by a note. False when nothing is left to take out.
    fn make_room(&mut self) -> bool {
        if let Some(next) = self.history.iter().skip(1).position(|m| m.role == "user") {
            self.history.drain(..next + 1);
            return true;
        }
        match self.history.iter_mut().find(|m| {
            m.role == "tool"
                && m.content
                    .iter()
                    .map(PreparedPart::retained_bytes)
                    .sum::<usize>()
                    > 200
        }) {
            Some(result) => {
                result.content = vec![PreparedPart::Text { text: "(This result was left out: the context was full. Run the tool again for less, e.g. a few lines.)".into() }];
                true
            }
            None => false,
        }
    }
}

fn message(role: &str, text: String) -> ChatMessage<Vec<PreparedPart>> {
    ChatMessage {
        role: role.into(),
        content: vec![PreparedPart::Text { text }],
    }
}

/// `git diff` of the files the last request changed (all changes when it changed none), or
/// `git status` when the folder is not a repository
fn diff(root: &std::path::Path, files: &[std::path::PathBuf]) -> String {
    let git = |args: &[&str], paths: &[std::path::PathBuf]| {
        std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .arg("--")
            .args(paths)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
    };
    match git(&["diff", "--no-color", "--stat", "--patch"], files) {
        Some(text) if !text.is_empty() => text.chars().take(20_000).collect(),
        Some(_) => "No uncommitted changes in these files.".into(),
        None => "Not a git repository (or git is not installed): /undo still restores the last request's files.".into(),
    }
}

/// The share of the context after which the next request first compacts the conversation
const AUTO_COMPACT_PCT: usize = 80;

/// What `/compact` asks the model for
const COMPACT_REQUEST: &str = "Summarize this conversation so the work can go on from the summary alone: the goal, what was decided, files read or changed (with paths), commands run and their results, and the open next steps. Be concise; use a short list. Do not call tools.";

/// The history as `role: text` blocks within `budget` characters for the compact request: long
/// messages keep their start, and when it all does not fit the newest messages are kept
fn compact_transcript(history: &[ChatMessage<Vec<PreparedPart>>], budget: usize) -> String {
    const MESSAGE_CHARS: usize = 1500;
    let mut blocks = Vec::new();
    let mut used = 0;
    for entry in history.iter().rev() {
        let text: String = entry
            .content
            .iter()
            .map(|part| match part {
                PreparedPart::Text { text } => text.as_str(),
                PreparedPart::Image { .. } => "[image]",
                PreparedPart::Audio { .. } => "[audio]",
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = text.trim();
        let text = match text.char_indices().nth(MESSAGE_CHARS) {
            Some((cut, _)) => format!("{}… ({} more characters)", &text[..cut], text.len() - cut),
            None => text.to_owned(),
        };
        let block = format!("{}: {text}", entry.role);
        if used + block.len() > budget && !blocks.is_empty() {
            blocks.push("(earlier messages left out)".to_owned());
            break;
        }
        used += block.len();
        blocks.push(block);
    }
    blocks.reverse();
    blocks.join("\n\n")
}

/// Below this context a written file or a long tool result often does not fit
const SMALL_CONTEXT: u32 = 8192;

/// "Context N", with what to do when N is too small for agent work
fn context_note(tokens: u32) -> String {
    if tokens < SMALL_CONTEXT {
        format!("Context {tokens} (small: long files will not fit; close other model processes or start with -c 16384)")
    } else {
        format!("Context {tokens}")
    }
}

fn system_prompt(dir: &std::path::Path, extra: Option<&str>) -> String {
    let shell = if cfg!(windows) { "PowerShell" } else { "sh" };
    let mut prompt = format!(
        "You are Cluaiz Agent, a coding assistant working on the user's computer through the cluaiz command line.\n\
         Working folder: {}\n\
         System: {} ({}); run_command uses {}.\n\n\
         - Use the tools to look at files and run commands instead of guessing. Never say you ran or read something you did not.\n\
         - Read a file before you edit it, and keep edits small. Use paths relative to the working folder.\n\
         - When the task is done, say briefly what you found or changed.",
        dir.display(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        shell,
    );
    // The project's own notes for agents
    if let Ok(notes) = std::fs::read_to_string(dir.join("AGENTS.md")) {
        let notes: String = notes.chars().take(MAX_PROJECT_NOTES).collect();
        prompt.push_str(&format!("\n\nThe project's AGENTS.md:\n{}", notes.trim()));
    }
    if let Some(extra) = extra.filter(|e| !e.trim().is_empty()) {
        prompt.push_str(&format!("\n\n{}", extra.trim()));
    }
    prompt
}

/// A canonical Windows path without the `\\?\` prefix (shown to the model and the user)
fn dunce(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(test)]
mod context_tests {
    use super::context_note;

    #[test]
    fn a_small_context_says_what_to_do() {
        assert_eq!(context_note(16384), "Context 16384");
        assert!(context_note(4096).contains("-c 16384"));
    }

    #[test]
    fn the_compact_request_carries_the_conversation_that_fits() {
        use super::{compact_transcript, message};
        let history = vec![
            message("user", "read the readme".into()),
            message("tool", "x".repeat(5000)),
            message("assistant", "Cluaiz is a local AI engine.".into()),
        ];
        let all = compact_transcript(&history, 10_000);
        assert!(all.starts_with("user: read the readme"));
        assert!(all.contains("(3500 more characters)"));
        assert!(all.ends_with("assistant: Cluaiz is a local AI engine."));
        let newest = compact_transcript(&history, 1000);
        assert!(newest.starts_with("(earlier messages left out)"));
        assert!(newest.ends_with("local AI engine."));
    }
}
