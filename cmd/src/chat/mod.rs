//! Chat in the terminal. A session is one loaded model plus the conversation; every turn is sent with
//! the same envelope and context rules as an API request (`cluaiz_core::ChatEnvelope`), so the CLI and
//! the API behave the same.

pub mod print;
pub mod repl;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cluaiz_core::{ActiveUserSettings, ChatEnvelope, ChatMessage, FeatureState, SamplerDefaults, Settings, ThinkingBudget};
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
        self.load.apply(&mut settings.optimization).map_err(|e| eyre!(e))?;
        Ok(settings)
    }
}

/// What one answer cost, as llama.cpp measured it
pub struct TurnReport {
    pub timings: Option<GenerationTimings>,
    pub thinking: Option<ThinkingReport>,
    pub cancelled: bool,
}

/// True while an answer is streaming: Ctrl+C then stops the answer instead of the program.
static GENERATING: AtomicBool = AtomicBool::new(false);

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
}

impl Session {
    /// Loads (or reuses) the model with the options' load flags.
    pub async fn open(host: Arc<EngineHost>, model_path: PathBuf, model_id: String, options: ChatOptions) -> Result<Self> {
        let settings = options.settings()?;
        let started = std::time::Instant::now();
        let _spin = crate::ui::spinner::Spinner::start("Loading model…");
        let plan = host.ensure_loaded(&model_id, &model_path, &settings).await.map_err(|e| eyre!("{}", e))?;
        drop(_spin);
        Ok(Self { host, model_id, model_path, plan, load_ms: started.elapsed().as_millis(), options, history: Vec::new() })
    }

    /// Switches to another model; the conversation is kept.
    pub async fn switch(&mut self, model_path: PathBuf, model_id: String) -> Result<()> {
        let settings = self.options.settings()?;
        let started = std::time::Instant::now();
        let _spin = crate::ui::spinner::Spinner::start("Switching model…");
        self.plan = self.host.ensure_loaded(&model_id, &model_path, &settings).await.map_err(|e| eyre!("{}", e))?;
        drop(_spin);
        self.load_ms = started.elapsed().as_millis();
        self.model_id = model_id;
        self.model_path = model_path;
        Ok(())
    }

    /// Sends `text` as the next user message and streams the answer into `out`.
    pub async fn send(&mut self, text: &str, out: &mut print::StreamPrinter) -> Result<TurnReport> {
        let settings = Settings::current();
        let opt = &settings.optimization;
        let n_ctx = cluaiz_core::active_context_tokens().unwrap_or(self.plan.context_window) as usize;

        let mut messages = Vec::with_capacity(self.history.len() + 2);
        if let Some(system) = self.options.system.as_ref().filter(|s| !s.is_empty()) {
            messages.push(ChatMessage::new("system", system.clone()));
        }
        messages.extend(self.history.iter().cloned());
        messages.push(ChatMessage::new("user", text));

        let reserve = cluaiz_core::answer_reserve(self.options.max_tokens, n_ctx);
        let dropped = cluaiz_core::trim_history(&mut messages, n_ctx, reserve, opt.context_shifting);
        if dropped > 0 {
            tracing::info!("[chat] dropped the oldest turns (~{} tokens) to fit {} tokens of context", dropped, n_ctx);
        }
        let prompt_tokens: usize = messages.iter().map(ChatMessage::estimated_tokens).sum();
        let max_tokens = cluaiz_core::answer_budget(n_ctx, prompt_tokens, self.options.max_tokens);

        let think = self.options.think.unwrap_or(opt.think_mode);
        let envelope = ChatEnvelope {
            messages,
            samplers: self.options.samplers.clone(),
            think_mode: think.name().to_string(),
            reasoning_effort: self.options.effort.unwrap_or(opt.reasoning_effort),
            reasoning_budget_message: self.options.budget_message.clone().or_else(|| opt.reasoning_budget_message.clone()),
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
        };

        // Tags from llama.cpp's template engine (read at load); no tags = the model has no thinking format
        let (start_tag, end_tag) = {
            let dna = self.host.dna.lock().await;
            let tag = |t: &str| Some(t.to_string()).filter(|t| !t.is_empty());
            (tag(&dna.think_tag_schema), tag(&dna.think_end_schema))
        };
        let no_thinking = think == FeatureState::Off || (start_tag.is_none() && end_tag.is_none());
        let mut filter = StreamingReasoningFilter::new(start_tag, end_tag, false, no_thinking);

        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
        GENERATING.store(true, Ordering::SeqCst);
        let result = self.stream(&envelope.to_prompt(), max_tokens, &mut filter, out).await;
        GENERATING.store(false, Ordering::SeqCst);
        let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.swap(false, Ordering::SeqCst);
        out.finish();
        let answer = result?;

        self.history.push(ChatMessage::new("user", text));
        self.history.push(ChatMessage::new("assistant", answer));
        Ok(TurnReport {
            timings: self.host.last_timings().await,
            thinking: self.host.last_thinking().await,
            cancelled,
        })
    }

    /// Streams one generation; returns the answer text (thinking excluded).
    async fn stream(&self, prompt: &str, max_tokens: usize, filter: &mut StreamingReasoningFilter, out: &mut print::StreamPrinter) -> Result<String> {
        let mut rx = match self.host.dispatch_stream(prompt, max_tokens).await {
            HostResponse::TokenStream(rx) => rx,
            HostResponse::Completed(text) => {
                out.content(&text);
                return Ok(text);
            }
            HostResponse::Error(e) => return Err(eyre!(e)),
        };
        let mut answer = String::new();
        let mut emit = |(content, reasoning): (Option<String>, Option<String>), out: &mut print::StreamPrinter| {
            if let Some(r) = reasoning {
                out.reasoning(&r);
            }
            if let Some(c) = content {
                out.content(&c);
                answer.push_str(&c);
            }
        };
        while let Some(token) = rx.recv().await {
            if let Some(error) = cluaiz_runtime::stream_error(&token) {
                return Err(eyre!("{}", error));
            }
            emit(filter.process_token(&token), out);
        }
        emit(filter.flush_final(), out);
        Ok(answer)
    }
}

/// `cluaiz run`: answers `prompt` once, or chats (`interactive`, or no prompt).
pub async fn start(model_path: PathBuf, model_id: String, options: ChatOptions, prompt: Option<String>, interactive: bool) -> Result<()> {
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

