//! Chat in the terminal. A session is one loaded model plus the conversation; every turn is sent with
//! the same envelope and context rules as an API request (`cluaiz_core::ChatEnvelope`), so the CLI and
//! the API behave the same.

pub mod markdown;
pub mod output;
pub mod print;
pub mod remote;
pub mod repl;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cluaiz_core::{
    ActiveUserSettings, ChatEnvelope, ChatMessage, FeatureState, SamplerDefaults, Settings,
    ThinkingBudget,
};
use cluaiz_models::StreamingReasoningFilter;
use cluaiz_plan::ExecutionPlan;
use cluaiz_runtime::{EngineHost, GenerationTimings, HostResponse, ThinkingReport};
use color_eyre::{eyre::eyre, Result};

use crate::cli::args::{LoadArgs, SampleArgs, ThinkArgs};

/// Everything a chat run can override; `None` = settings.json (then the model's defaults).
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub load: LoadArgs,
    pub samplers: SamplerDefaults,
    pub think: Option<FeatureState>,
    pub effort: Option<ThinkingBudget>,
    pub budget_message: Option<String>,
    /// Most tokens per answer (llama.cpp -n); None = what the context leaves
    pub max_tokens: Option<usize>,
    pub system: Option<String>,
    pub show_thinking: bool,
    /// Speed after each answer, full plan on load
    pub verbose: bool,
    /// Load the model in this process even when `cluaiz serve` is running
    pub local: bool,
}

impl ChatOptions {
    pub fn new(load: &LoadArgs, sample: &SampleArgs, think: &ThinkArgs) -> Self {
        Self {
            load: load.clone(),
            samplers: sample.samplers(),
            think: think.think,
            effort: think.effort(),
            budget_message: think.reasoning_budget_message.clone(),
            show_thinking: !think.hide_thinking,
            ..Default::default()
        }
    }

    /// settings.json with this run's load flags on top
    pub fn settings(&self) -> Result<ActiveUserSettings> {
        let mut settings = Settings::current();
        self.load
            .apply(&mut settings.optimization)
            .map_err(|e| eyre!(e))?;
        Ok(settings)
    }
}

/// What one answer cost, as llama.cpp measured it
pub struct TurnReport {
    pub timings: Option<GenerationTimings>,
    pub thinking: Option<ThinkingReport>,
    pub cancelled: bool,
    pub stop: &'static str,
    /// Tokens in the context when the answer ended, of the context size
    pub context: Option<(usize, usize)>,
    /// Oldest messages the Engine left out so the request fitted
    pub dropped: usize,
}

/// True while an answer is streaming: Ctrl+C then stops the answer instead of the program.
pub static GENERATING: AtomicBool = AtomicBool::new(false);

/// Ctrl+C stops the running answer; at the prompt it exits (llama-cli behaviour).
pub fn install_interrupt_handler() {
    let _ = ctrlc::set_handler(|| {
        if GENERATING.load(Ordering::SeqCst) {
            cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
        } else {
            println!();
            std::process::exit(130);
        }
    });
}

pub struct Session {
    pub host: Arc<EngineHost>,
    pub model_id: String,
    pub model_path: PathBuf,
    pub plan: ExecutionPlan,
    pub load_ms: u128,
    pub options: ChatOptions,
    pub history: Vec<ChatMessage>,
    /// The running server that holds the model, when this session uses it
    pub remote: Option<remote::Remote>,
}

impl Session {
    /// Loads (or reuses) the model with the options' load flags.
    pub async fn open(
        host: Arc<EngineHost>,
        model_path: PathBuf,
        model_id: String,
        options: ChatOptions,
    ) -> Result<Self> {
        let started = std::time::Instant::now();
        // A running server already holds models: use it, unless this run asks for its own load
        // (--local, or load flags such as -c / -ngl that the server's copy would not follow)
        let own_load = format!("{:?}", options.load) != format!("{:?}", LoadArgs::default());
        if !options.local && !own_load {
            if let Some((remote, plan)) = remote::Remote::attach(&model_id).await {
                return Ok(Self {
                    host,
                    model_id,
                    model_path,
                    plan,
                    load_ms: started.elapsed().as_millis(),
                    options,
                    history: Vec::new(),
                    remote: Some(remote),
                });
            }
        }
        let settings = options.settings()?;
        let _spin = crate::ui::spinner::Spinner::start("Loading model…");
        let plan = host
            .ensure_loaded(&model_id, &model_path, &settings)
            .await
            .map_err(|e| eyre!("{}", e))?;
        drop(_spin);
        Ok(Self {
            host,
            model_id,
            model_path,
            plan,
            load_ms: started.elapsed().as_millis(),
            options,
            history: Vec::new(),
            remote: None,
        })
    }

    /// Switches to another model; the conversation is kept.
    pub async fn switch(&mut self, model_path: PathBuf, model_id: String) -> Result<()> {
        if self.remote.is_some() {
            let started = std::time::Instant::now();
            let (remote, plan) = remote::Remote::attach(&model_id)
                .await
                .ok_or_else(|| eyre!("the running server could not load {model_id}"))?;
            self.remote = Some(remote);
            self.plan = plan;
            self.load_ms = started.elapsed().as_millis();
            self.model_id = model_id;
            self.model_path = model_path;
            return Ok(());
        }
        let settings = self.options.settings()?;
        let started = std::time::Instant::now();
        let _spin = crate::ui::spinner::Spinner::start("Switching model…");
        self.plan = self
            .host
            .ensure_loaded(&model_id, &model_path, &settings)
            .await
            .map_err(|e| eyre!("{}", e))?;
        drop(_spin);
        self.load_ms = started.elapsed().as_millis();
        self.model_id = model_id;
        self.model_path = model_path;
        Ok(())
    }

    /// Sends `text` as the next user message and streams the answer into `out`.
    pub async fn send(&mut self, text: &str, out: &mut print::StreamPrinter) -> Result<TurnReport> {
        let mut messages = Vec::with_capacity(self.history.len() + 2);
        if let Some(system) = self.options.system.as_ref().filter(|s| !s.is_empty()) {
            messages.push(ChatMessage::new("system", system.clone()));
        }
        messages.extend(self.history.iter().cloned());
        messages.push(ChatMessage::new("user", text));

        let (answer, report) = self.generate(messages, None, out).await?;
        self.history.push(ChatMessage::new("user", text));
        self.history.push(ChatMessage::new("assistant", answer));
        Ok(report)
    }

    /// One answer to `messages` (system prompt and history included), streamed into `out`. `tools`
    /// are function schemas the model may call, rendered by its own template as for an API request's
    /// `tools`; its calls come back in the answer as `<tool_call>` blocks. Returns the answer without
    /// its thinking.
    pub async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<&serde_json::Value>,
        out: &mut dyn output::Output,
    ) -> Result<(String, TurnReport)> {
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
        let messages = messages
            .into_iter()
            .map(|message| ChatMessage {
                role: message.role,
                content: vec![cluaiz_core::media::PreparedPart::Text {
                    text: message.content,
                }],
            })
            .collect();
        self.generate_prepared(messages, tools, out).await
    }

    /// Text and attachments use the same native request, options, tool schemas and output filter.
    pub async fn generate_prepared(
        &self,
        messages: Vec<ChatMessage<Vec<cluaiz_core::media::PreparedPart>>>,
        tools: Option<&serde_json::Value>,
        out: &mut dyn output::Output,
    ) -> Result<(String, TurnReport)> {
        if let Some(remote) = &self.remote {
            return remote.generate(&self.model_id, &self.options, messages, tools, out).await;
        }
        let settings = self.options.settings()?;
        let opt = &settings.optimization;
        let n_ctx =
            cluaiz_core::active_context_tokens().unwrap_or(self.plan.context_window) as usize;

        // No max_tokens: until the model ends or the context does; the engine fits the history
        let max_tokens = self.options.max_tokens.unwrap_or(n_ctx).min(n_ctx);

        let think = self.options.think.unwrap_or(opt.think_mode);
        let envelope = ChatEnvelope {
            messages,
            samplers: self.options.samplers.clone(),
            think_mode: think.name().to_string(),
            reasoning_effort: self.options.effort.unwrap_or(opt.reasoning_effort),
            reasoning_budget_message: self
                .options
                .budget_message
                .clone()
                .or_else(|| opt.reasoning_budget_message.clone()),
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
            tools: tools.cloned(),
        };

        // Tags from llama.cpp's template engine (read at load); no tags = the model has no thinking format
        let (start_tag, end_tag) = {
            let dna = self.host.dna.lock().await;
            let tag = |t: &str| Some(t.to_string()).filter(|t| !t.is_empty());
            (tag(&dna.think_tag_schema), tag(&dna.think_end_schema))
        };
        let no_thinking = think == FeatureState::Off || (start_tag.is_none() && end_tag.is_none());
        let mut filter = StreamingReasoningFilter::new(start_tag, end_tag, false, no_thinking);

        cluaiz_core::live::start();
        GENERATING.store(true, Ordering::SeqCst);
        let response = self
            .host
            .dispatch_prepared(
                envelope,
                max_tokens,
                &self.model_path,
                &settings,
                None,
                None,
            )
            .await;
        let result = self.stream(response, &mut filter, out).await;
        GENERATING.store(false, Ordering::SeqCst);
        let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.swap(false, Ordering::SeqCst);
        out.finish();
        let answer = result?;
        let context = self.host.last_context().await;
        let report = TurnReport {
            timings: self.host.last_timings().await,
            thinking: self.host.last_thinking().await,
            cancelled,
            stop: context.as_ref().map(|report| report.stop).unwrap_or("unknown"),
            context: context.as_ref().map(|report| (report.n_tokens, report.n_ctx)),
            dropped: context.as_ref().map_or(0, |report| report.dropped_messages),
        };
        Ok((answer, report))
    }

    /// Streams one generation; returns the answer text (thinking excluded).
    async fn stream(
        &self,
        response: HostResponse,
        filter: &mut StreamingReasoningFilter,
        out: &mut dyn output::Output,
    ) -> Result<String> {
        let mut rx = match response {
            HostResponse::TokenStream(rx) => rx,
            HostResponse::Completed(text) => {
                out.content(&text);
                return Ok(text);
            }
            HostResponse::Error(e) => return Err(eyre!(e)),
        };
        let mut answer = String::new();
        let mut emit = |(content, reasoning): (Option<String>, Option<String>),
                        out: &mut dyn output::Output| {
            if let Some(r) = reasoning {
                out.reasoning(&r);
            }
            if let Some(c) = content {
                out.content(&c);
                answer.push_str(&c);
            }
        };
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(120));
        loop {
            tokio::select! {
                token = rx.recv() => {
                    let Some(token) = token else { break };
                    if let Some(error) = cluaiz_runtime::stream_error(&token) {
                        return Err(eyre!("{}", error));
                    }
                    emit(filter.process_token(&token), out);
                }
                _ = tick.tick() => out.tick(),
            }
        }
        emit(filter.flush_final(), out);
        Ok(answer)
    }
}

/// `cluaiz run`: answers `prompt` once, or chats (`interactive`, or no prompt).
pub async fn start(
    model_path: PathBuf,
    model_id: String,
    options: ChatOptions,
    prompt: Option<String>,
    interactive: bool,
) -> Result<()> {
    install_interrupt_handler();
    let one_shot = prompt.is_some() && !interactive;
    let host = Arc::new(EngineHost::new());
    let mut session = Session::open(host, model_path, model_id, options).await?;
    if !one_shot || session.options.verbose {
        print::plan(&session).await;
    }
    if one_shot {
        let prompt = prompt.unwrap_or_default();
        let mut out = print::StreamPrinter::new(session.options.show_thinking);
        let report = session.send(&prompt, &mut out).await?;
        if session.options.verbose {
            print::stats(&report);
        }
        return Ok(());
    }
    let mut state = crate::core::state::AppState::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    repl::run(&mut session, &mut state, &tx, &mut rx, prompt).await
}
