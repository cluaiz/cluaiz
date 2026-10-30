//! `cluaiz agent`: a coding agent in the terminal. The model works in a folder with tools (read,
//! search, edit files, run commands): each answer that calls tools gets their results and goes on,
//! until it answers without a call. Tools are offered the way an API request's `tools` are (the
//! model's own template renders them, `<tool_call>` blocks come back), on the same loaded model and
//! settings as `cluaiz run`.

mod commands;
mod completion;
mod display;
mod terminal;
mod terminal_state;
pub mod tools;
mod validation;

use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Allow every file change and command without asking (destructive commands stay refused)
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Keep chatting after doing the task
    #[arg(short = 'i', long)]
    pub interactive: bool,
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

pub async fn execute(args: AgentArgs, verbose: bool) -> Result<()> {
    let screen_enabled = args.screen_enabled();
    if matches!(args.ui, UiMode::Tui) && !screen_enabled {
        return Err(eyre!(
            "Interactive terminal required; use --ui plain for redirected output"
        ));
    }
    let name = args
        .model
        .clone()
        .or_else(models::default_chat_model)
        .ok_or_else(|| eyre!("no model given and no active chat model (see `cluaiz list`, `cluaiz agent -m <model>`)"))?;
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

    let mut options = ChatOptions::new(&args.load, &args.sample, &args.think);
    options.max_tokens = args.n_predict;
    options.verbose = verbose;
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
    };
    if screen_enabled {
        return terminal::run(agent, task, interactive).await;
    }
    if interactive {
        eprintln!("{} {}", "Working in".dimmed(), agent.tools.root().display());
        eprintln!("{}", "Type a task, /? for help, /bye to exit.".dimmed());
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
    history: Vec<ChatMessage>,
    display: Display,
}

impl Agent {
    fn status(&self) -> String {
        format!(
            "Context {} tokens · {} · /help  /theme  @files",
            self.session.plan.context_window,
            if self.tools.allow_all {
                "Session changes allowed"
            } else {
                "Ask before changes"
            }
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
                "Conversation cleared.".into()
            }
            "/yes" => {
                self.tools.allow_all = true;
                "Changes and commands now run without asking (existing hard denials still apply)."
                    .into()
            }
            "/ask" => {
                self.tools.require_approval();
                "Each change and command asks first.".into()
            }
            _ if text.starts_with('/') => format!("Unknown command {text}; use /help"),
            _ => return None,
        };
        self.display.emit(Event::Message("Agent".into(), message));
        Some(false)
    }

    /// One request: answers and tool calls until an answer has no call.
    async fn turn(&mut self, text: &str) -> Result<()> {
        if self.display.closing() {
            return Ok(());
        }
        self.display.begin_turn();
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, std::sync::atomic::Ordering::SeqCst);
        self.history.push(ChatMessage::new("user", text));
        let schemas = Tools::schemas();
        for _ in 0..MAX_STEPS {
            let (answer, report) = loop {
                let mut messages = Vec::with_capacity(self.history.len() + 1);
                messages.push(ChatMessage::new("system", self.system.clone()));
                messages.extend(self.history.iter().cloned());
                let mut out = self.display.output(self.session.options.show_thinking);
                match self
                    .session
                    .generate(messages, Some(&schemas), &mut *out)
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
            if report.cancelled || self.display.interrupted() {
                let said = answer
                    .split(cluaiz_core::tool_calls::OPEN)
                    .next()
                    .unwrap_or_default();
                self.history.push(ChatMessage::new("assistant", said));
                self.display.emit(Event::Message(
                    "Stopped".into(),
                    "Generation cancelled.".into(),
                ));
                return Ok(());
            }
            let (said, calls) =
                validation::calls(&answer, report.stop).map_err(|error| eyre!(error))?;
            if calls.is_empty() {
                self.history.push(ChatMessage::new("assistant", said));
                return Ok(());
            }
            // Kept in the Engine's form, as the API keeps a conversation with calls
            let mut kept = said;
            for (name, arguments) in &calls {
                kept.push('\n');
                kept.push_str(&cluaiz_core::tool_calls::canonical_call(name, arguments));
            }
            self.history
                .push(ChatMessage::new("assistant", kept.trim().to_string()));
            for (name, arguments) in &calls {
                if self.display.closing() || self.display.interrupted() {
                    return Ok(());
                }
                let result = self.tools.run(name, arguments).await;
                self.history.push(ChatMessage::new("tool", result));
                if self.tools.cancelled {
                    return Ok(());
                }
            }
        }
        self.display.emit(Event::Message(
            "Stopped".into(),
            format!("Stopped after {MAX_STEPS} steps; say how to go on."),
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
        match self
            .history
            .iter_mut()
            .find(|m| m.role == "tool" && m.content.len() > 200)
        {
            Some(result) => {
                result.content = "(This result was left out: the context was full. Run the tool again for less, e.g. a few lines.)".into();
                true
            }
            None => false,
        }
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
