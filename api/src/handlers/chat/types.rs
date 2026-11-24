use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, LazyLock, RwLock};

/// Thread-safe multi-stream controller signals
#[derive(Clone, Default)]
pub struct StreamSignals {
    pub cancel: Arc<AtomicBool>,
    pub skip_reasoning: Arc<AtomicBool>,
    pub reasoning_supported: bool,
}

/// Global active streaming sessions registry (Keyed by unique `chatcmpl-...` stream_id)
pub static ACTIVE_STREAMS: LazyLock<RwLock<HashMap<String, StreamSignals>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// `max_steps` when it is a whole number of zero or more; anything else leaves the default
fn steps_or_default<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<usize>, D::Error> {
    Ok(Value::deserialize(deserializer)?.as_u64().and_then(|steps| usize::try_from(steps).ok()))
}

/// Fallback maximum token generation limit when not explicitly specified
pub const DEFAULT_MAX_TOKENS: usize = 2048;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemporaryChatMode {
    Lite,
    Strict,
}

/// OpenAI's `stop` field: a single string or an array of strings
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum StopList {
    One(String),
    Many(Vec<String>),
}

impl StopList {
    /// The stop strings, empty ones dropped
    pub fn into_vec(self) -> Vec<String> {
        match self {
            StopList::One(s) => vec![s],
            StopList::Many(v) => v,
        }
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect()
    }
}

#[derive(Deserialize, Clone)]
pub struct ExternalChatRequest {
    pub model: Option<String>,
    pub messages: Vec<ExternalMessage>,
    #[serde(default)]
    pub stream: bool,
    /// OpenAI `stream_options`; `include_usage: true` asks for the final usage chunk
    #[serde(default)]
    pub stream_options: Option<serde_json::Value>,
    /// Opt-in named SSE events alongside the unchanged completion chunks.
    #[serde(default)]
    pub return_progress: bool,
    #[serde(default)]
    pub temporary_chat: Option<TemporaryChatMode>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub think_mode: Option<serde_json::Value>,
    /// Thinking budget: auto, low, medium, high, max or a number of tokens. Missing = the user's default setting.
    pub reasoning_effort: Option<cluaiz_core::ThinkingBudget>,
    /// Text written before the end-of-thinking tag when the budget runs out (llama.cpp
    /// --reasoning-budget-message). Missing = the user's default setting; "" = no text.
    pub reasoning_budget_message: Option<String>,
    pub skip_reasoning: Option<bool>,
    #[serde(default, deserialize_with = "crate::utils::keep_alive_minutes")]
    pub keep_alive: Option<i32>,
    pub min_p: Option<f32>,
    pub repetition_penalty: Option<f32>,

    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    /// OpenAI's current name for the answer limit; `max_tokens` wins when both are sent
    pub max_completion_tokens: Option<usize>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    /// OpenAI `stop`: one string or a list of strings
    pub stop: Option<StopList>,
    pub seed: Option<i64>,
    pub response_format: Option<serde_json::Value>,
    pub chat_template_kwargs: Option<serde_json::Value>,
    pub grammar: Option<String>,
    /// OpenAI `logit_bias`: token id (as a string) -> bias from -100 to 100. Values stay raw JSON so
    /// a wrong one is a 400 naming the field (`logit_bias_pairs`), not a body parse error
    pub logit_bias: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// This request's tool approval mode (`full_access`, `sandboxed`, `strict`); missing = the
    /// global `agent_security_mode`. Only as strict or stricter than global (`approval_mode_id`).
    /// Raw JSON, so a wrong value is a 400 naming the field, not a body parse error
    pub approval_mode: Option<serde_json::Value>,
    /// OpenAI `n`: answers per request. An installed model gives one; `n` above 1 is refused there
    pub n: Option<u64>,
    /// OpenAI `logprobs` / `top_logprobs`: an installed model returns no token probabilities, so
    /// asking for them is refused there
    pub logprobs: Option<bool>,
    pub top_logprobs: Option<u64>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: Option<serde_json::Value>,
    /// Maximum tool-call generations in one turn (`0` = unlimited; default `100`). A value that is
    /// not a whole number of zero or more is ignored, so the default applies
    #[serde(default, deserialize_with = "steps_or_default")]
    pub max_steps: Option<usize>,
    /// false: installed tools that fit the message are not suggested (default true)
    pub tool_search: Option<bool>,
    /// How the KV cache is matched to the new prompt: "prefix" (only the common prefix is kept),
    /// or "chunk" (llama-server `--cache-reuse`: shared chunks after it are moved, which keeps
    /// their old context). Default: "chunk" only when the Engine dropped old turns to fit.
    #[serde(default)]
    pub kv_reuse: Option<String>,
    /// Debug: the engine report's context carries the rendered prompt (template tests)
    #[serde(default)]
    pub return_prompt: Option<bool>,
    /// "recent" (default): the whole history, the oldest turns dropped when it does not fit;
    /// "none": only the system prompt and the current message
    #[serde(default)]
    pub history_mode: Option<String>,
    /// "client" (default): the client sends the whole conversation. "engine": the client sends
    /// only the new messages and the Engine puts the chat's logged transcript in front of them
    /// (needs session_id)
    #[serde(default)]
    pub history: Option<String>,
    /// "engine" (default): the Engine keeps the chat on disk. "external": the app keeps it; the
    /// Engine holds it in RAM only (no event log, ledger or saved KV on disk) and sends the new
    /// events with each answer (`memory_events`). "none": nothing is kept (like temporary_chat).
    #[serde(default)]
    pub storage: Option<String>,
    /// The app's end user (OpenAI's `user`): notes the model keeps in this chat belong to that
    /// user, and only that user's notes reach the model. Without it: the Engine's own user.
    #[serde(default)]
    pub user: Option<String>,
    /// Planner fields for this request only (recent_turns_min, evidence_cap_pct,
    /// suggestions_cap_pct, margin_pct, compact_at_pct, compact_to_pct); unknown or invalid ones are ignored
    #[serde(default)]
    pub context_policy: Option<serde_json::Value>,
    /// Set by `POST /v1/chat/{id}/context/preview`: plan only, change nothing
    #[serde(skip)]
    pub preview: bool,
    /// Set by Ollama's `POST /api/generate` and `POST /api/chat`: only the client's messages and
    /// the client's own function tools go to the model template. No workspace agent files, notes,
    /// tool suggestions, Engine functions or saved flows are added (Ollama renders only what the
    /// client sent). Never read from the body, so `/v1/chat/completions` is not changed by it.
    #[serde(skip)]
    pub plain_prompt: bool,
    /// llama.cpp load settings for this request (llama.cpp names). Missing = auto (planner + user settings).
    /// A value different from the loaded model's reloads it; n_gpu_layers / n_cpu_moe / offload_kqv bypass the planner.
    #[serde(default)]
    pub llama: Option<cluaiz_core::LlamaLoadParams>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Array(Vec<ContentPart>),
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

impl MessageContent {
    /// Text used by skill matching/telemetry, with no media fetch or fake path tokens.
    /// The text, borrowed when the content is plain text
    pub fn text_ref(&self) -> std::borrow::Cow<'_, str> {
        match self {
            MessageContent::Text(text) => std::borrow::Cow::Borrowed(text),
            MessageContent::Array(_) => std::borrow::Cow::Owned(self.text_content()),
        }
    }

    pub fn text_content(&self) -> String {
        match self {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Array(parts) => {
                let mut combined = String::new();
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        combined.push_str(text);
                        combined.push('\n');
                    }
                }
                combined.trim_end().to_string()
            }
        }
    }

    pub fn has_media(&self) -> bool {
        matches!(self, Self::Array(parts) if parts.iter().any(|part| !matches!(part, ContentPart::Text { .. })))
    }

    /// Preserve ordered transport parts; decoding belongs to cluaiz_media.
    pub fn input_parts(&self) -> anyhow::Result<Vec<cluaiz_core::media::InputPart>> {
        use cluaiz_core::media::InputPart;
        match self {
            Self::Text(text) => Ok(vec![InputPart::Text { text: text.clone() }]),
            Self::Array(parts) => parts
                .iter()
                .map(|part| {
                    Ok(match part {
                        ContentPart::Text { text } => InputPart::Text { text: text.clone() },
                        ContentPart::ImageUrl { image_url } => InputPart::Image {
                            source: image_url.url.clone(),
                        },
                        ContentPart::AudioUrl { audio_url } => InputPart::Audio {
                            source: audio_url.url.clone(),
                        },
                        ContentPart::InputAudio { input_audio } => InputPart::Audio {
                            source: format!(
                                "data:{};base64,{}",
                                cluaiz_media::formats::audio_mime(&input_audio.format)?,
                                input_audio.data
                            ),
                        },
                        ContentPart::FileUrl { file_url } => InputPart::Document {
                            source: file_url.url.clone(),
                        },
                        ContentPart::VideoUrl { video_url } => InputPart::Video {
                            source: video_url.url.clone(),
                            include_audio: video_url.include_audio,
                        },
                    })
                })
                .collect(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: MediaUrlContent },
    #[serde(rename = "audio_url")]
    AudioUrl { audio_url: MediaUrlContent },
    #[serde(rename = "input_audio")]
    InputAudio { input_audio: InputAudioContent },
    #[serde(rename = "file_url")]
    FileUrl { file_url: MediaUrlContent },
    #[serde(rename = "video_url")]
    VideoUrl { video_url: VideoUrlContent },
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VideoUrlContent {
    pub url: String,
    #[serde(default)]
    pub include_audio: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct InputAudioContent {
    pub data: String,
    #[serde(default = "default_audio_format")]
    pub format: String,
}

fn default_audio_format() -> String {
    "wav".to_string()
}

#[derive(Deserialize, Clone, Debug)]
pub struct MediaUrlContent {
    pub url: String,
}

#[derive(Deserialize, Clone, Default)]
pub struct ExternalMessage {
    pub role: String,
    #[serde(default)]
    pub content: MessageContent,
    /// The client's id for the message (optional): the chat's event log matches messages by it
    #[serde(default)]
    pub id: Option<String>,
    /// OpenAI tool fields: accepted and logged as sent
    #[serde(default)]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

/// API error type of an engine error: llama-server's `exceed_context_size_error` (HTTP 400) when the
/// conversation does not fit the context even after context shifting, else `engine_error`.
pub fn engine_error_type(message: &str) -> &'static str {
    if message.contains("exceeds the available context size") {
        "exceed_context_size_error"
    } else {
        "engine_error"
    }
}

/// What the engine really did for the last generation: llama.cpp's own timings (same numbers as
/// llama-cli / llama-server) and the llama.cpp load values of the loaded model.
pub async fn engine_report(host: &cluaiz_runtime::EngineHost) -> Value {
    let plan = host.current_plan.lock().await.clone();
    let settings = host.loaded_settings.lock().await.clone();
    let physical_cores = cluaiz_hw::HardwareProber::current().cpu.physical_cores;
    json!({
        "timings": host.last_timings().await,
        "thinking": host.last_thinking().await,
        "context": host.last_context().await,
        "load": plan.map(|p| json!({
            "placement": format!("{:?}", p.selected_tier),
            "n_ctx": p.context_window,
            "n_gpu_layers": p.n_gpu_layers,
            "total_layers": p.total_layers,
            "n_cpu_moe": p.n_cpu_moe,
            "offload_kqv": p.kv_offload,
            "op_offload": p.op_offload,
            "cache_type": format!("{:?}", p.kv_cache_type),
            "planner": p.explanation,
            "settings": settings.as_ref().map(|s| {
                let (n_threads, n_threads_batch) = s.optimization.resolved_threads(physical_cores);
                json!({
                    "n_batch": s.optimization.resolved_batch_size(),
                    "n_ubatch": s.optimization.resolved_ubatch_size(),
                    "n_threads": n_threads,
                    "n_threads_batch": n_threads_batch,
                    "flash_attn": format!("{:?}", s.optimization.flash_attention),
                    "use_mmap": p.use_mmap(s),
                })
            }),
        })),
    })
}

type CachedModelHeader = Option<(std::time::Instant, Vec<Value>)>;
static CACHED_MODEL_HEADER_INFO: LazyLock<RwLock<CachedModelHeader>> =
    LazyLock::new(|| RwLock::new(None));

/// Helper to fetch real model header and hardware slot allocations (cached for fast response)
pub fn generate_model_header_info() -> Vec<Value> {
    if let Ok(guard) = CACHED_MODEL_HEADER_INFO.read() {
        if let Some((ts, ref cached)) = *guard {
            if ts.elapsed().as_secs() < 30 {
                return cached.clone();
            }
        }
    }

    let installed = cluaiz_models::registry::InstalledStateRegistry::load();
    let mut loaded_models = Vec::new();

    for (model_id, entry) in installed.installed_models {
        let think_start = entry.metadata.think_start_tag.unwrap_or_default();
        let think_close = entry.metadata.think_end_tag.unwrap_or_default();
        let context_window_total =
            cluaiz_models::StructuralDNA::parse_context_string(&entry.metadata.context_window);

        // Allocated = what llama.cpp created, only for the model that is loaded (0 otherwise)
        let is_loaded = cluaiz_core::active_model_path()
            .map(|p| {
                !entry.local_dir.is_empty()
                    && std::path::Path::new(&p).starts_with(&entry.local_dir)
            })
            .unwrap_or(false);
        let allocated_ctx = if is_loaded {
            cluaiz_core::active_context_tokens().unwrap_or(0)
        } else {
            0
        };

        loaded_models.push(json!({
            "model_id": model_id,
            "category": entry.category,
            "context_window_total": context_window_total,
            "context_window_allocated": allocated_ctx,
            "think_start_tag": think_start,
            "think_close_tag": think_close,
            "local_dir": entry.local_dir
        }));
    }

    if let Ok(mut guard) = CACHED_MODEL_HEADER_INFO.write() {
        *guard = Some((std::time::Instant::now(), loaded_models.clone()));
    }

    loaded_models
}

impl ExternalChatRequest {
    /// The answer limit: `max_tokens`, else OpenAI's newer `max_completion_tokens`
    pub fn answer_limit(&self) -> Option<usize> {
        self.max_tokens.or(self.max_completion_tokens)
    }

    /// The first OpenAI field an installed model cannot honour (field name, message), so the
    /// caller gets a 400 instead of an answer that silently ignored it. Providers are not checked.
    pub fn unsupported_on_installed_model(&self) -> Option<(&'static str, String)> {
        if let Some(n) = self.n.filter(|n| *n > 1) {
            return Some(("n", format!("n={n} is not supported on installed models: one answer per request (send n=1 or leave it out)")));
        }
        if self.logprobs == Some(true) {
            return Some(("logprobs", "logprobs is not supported on installed models yet (send logprobs=false or leave it out)".into()));
        }
        if self.top_logprobs.is_some_and(|k| k > 0) {
            return Some(("top_logprobs", "top_logprobs is not supported on installed models yet (leave it out)".into()));
        }
        None
    }

    /// The approval mode this request's tool calls run in (`cluaiz_security::chat_security_mode`
    /// with the global mode), or an error message for a 400 naming `approval_mode`
    pub fn approval_mode_id(&self, global: &str) -> Result<&'static str, String> {
        let requested = match &self.approval_mode {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(mode)) => Some(mode.as_str()),
            Some(other) => return Err(format!("approval_mode must be a string, got {other}")),
        };
        cluaiz_security::chat_security_mode(global, requested)
    }

    /// `logit_bias` as (token id, bias) pairs, sorted by id; missing or `{}` = none. A key that is
    /// not a token id or a value outside -100..100 (OpenAI's range) is an error message for a 400.
    /// Ids past the model's vocabulary are checked once the model is known (`check_vocabulary`)
    pub fn logit_bias_pairs(&self) -> Result<Vec<(i32, f32)>, String> {
        let Some(map) = &self.logit_bias else { return Ok(Vec::new()) };
        let mut pairs = Vec::with_capacity(map.len());
        for (key, value) in map {
            let token = key.trim().parse::<i32>().ok().filter(|id| *id >= 0).ok_or_else(|| {
                format!("logit_bias key '{key}' is not a token id (send token ids as strings, like {{\"1234\": -100}})")
            })?;
            let bias = value
                .as_f64()
                .filter(|b| (-LOGIT_BIAS_LIMIT..=LOGIT_BIAS_LIMIT).contains(b))
                .ok_or_else(|| format!("logit_bias value for token {token} must be a number from -100 to 100, got {value}"))?;
            pairs.push((token, bias as f32));
        }
        pairs.sort_by_key(|(token, _)| *token);
        Ok(pairs)
    }
}

/// OpenAI's `logit_bias` range: -100 bans a token, 100 makes it the only choice
const LOGIT_BIAS_LIMIT: f64 = 100.0;

/// The first `logit_bias` id the model's vocabulary does not have, as a message for a 400
pub fn check_vocabulary(pairs: &[(i32, f32)], vocabulary: u64) -> Result<(), String> {
    match pairs.iter().find(|(token, _)| *token as u64 >= vocabulary) {
        Some((token, _)) => Err(format!("logit_bias token {token} is not in this model's vocabulary ({vocabulary} tokens, ids 0 to {})", vocabulary.saturating_sub(1))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod openai_field_tests {
    use super::*;

    fn parse(extra: &str) -> ExternalChatRequest {
        serde_json::from_str(&format!(r#"{{"messages":[{{"role":"user","content":"hi"}}]{extra}}}"#)).unwrap()
    }

    #[test]
    fn stop_takes_a_string_or_a_list() {
        assert_eq!(parse(r#","stop":"5""#).stop.unwrap().into_vec(), vec!["5"]);
        assert_eq!(parse(r#","stop":["a","","b"]"#).stop.unwrap().into_vec(), vec!["a", "b"]);
        assert!(parse("").stop.is_none());
    }

    #[test]
    fn max_completion_tokens_is_the_answer_limit_unless_max_tokens_is_sent() {
        assert_eq!(parse(r#","max_completion_tokens":5"#).answer_limit(), Some(5));
        assert_eq!(parse(r#","max_completion_tokens":5,"max_tokens":9"#).answer_limit(), Some(9));
        assert_eq!(parse("").answer_limit(), None);
    }

    #[test]
    fn fields_an_installed_model_cannot_honour_are_named() {
        let field = |extra: &str| parse(extra).unsupported_on_installed_model().map(|(f, _)| f);
        assert_eq!(field(""), None);
        assert_eq!(field(r#","n":1,"logprobs":false,"top_logprobs":0,"logit_bias":{}"#), None);
        assert_eq!(field(r#","n":2"#), Some("n"));
        assert_eq!(field(r#","logprobs":true"#), Some("logprobs"));
        assert_eq!(field(r#","top_logprobs":3"#), Some("top_logprobs"));
        assert_eq!(field(r#","logit_bias":{"50256":-100}"#), None);
    }

    #[test]
    fn approval_mode_tightens_and_names_the_field() {
        assert_eq!(parse("").approval_mode_id("full_access"), Ok("full_access"));
        assert_eq!(parse(r#","approval_mode":null"#).approval_mode_id("sandboxed"), Ok("sandboxed"));
        assert_eq!(parse(r#","approval_mode":"strict""#).approval_mode_id("full_access"), Ok("strict"));
        assert_eq!(parse(r#","approval_mode":"full_access""#).approval_mode_id("strict"), Ok("strict"));
        for bad in [r#""yolo""#, "1", "true", "[]"] {
            let error = parse(&format!(r#","approval_mode":{bad}"#)).approval_mode_id("sandboxed").unwrap_err();
            assert!(error.contains("approval_mode"), "{error}");
        }
    }

    #[test]
    fn logit_bias_takes_token_ids_and_openai_range() {
        assert_eq!(parse("").logit_bias_pairs(), Ok(vec![]));
        assert_eq!(parse(r#","logit_bias":{}"#).logit_bias_pairs(), Ok(vec![]));
        assert_eq!(parse(r#","logit_bias":{"9":100,"3":-100,"5":0.5}"#).logit_bias_pairs(), Ok(vec![(3, -100.0), (5, 0.5), (9, 100.0)]));
        for bad in [r#"{"abc":1}"#, r#"{"-1":1}"#, r#"{"1.5":1}"#, r#"{"5":101}"#, r#"{"5":-100.5}"#, r#"{"5":"x"}"#, r#"{"5":null}"#] {
            assert!(parse(&format!(r#","logit_bias":{bad}"#)).logit_bias_pairs().is_err(), "{bad}");
        }
        assert_eq!(check_vocabulary(&[(3, 1.0), (9, 1.0)], 10), Ok(()));
        assert!(check_vocabulary(&[(3, 1.0), (10, 1.0)], 10).is_err());
    }
}
