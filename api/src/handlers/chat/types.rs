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

#[derive(Deserialize)]
pub struct StreamControlRequest {
    pub stream_id: String,
    pub reason: Option<String>,
}

/// Fallback maximum token generation limit when not explicitly specified
pub const DEFAULT_MAX_TOKENS: usize = 2048;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemporaryChatMode {
    Lite,
    Strict,
}

#[derive(Deserialize, Clone)]
pub struct ExternalChatRequest {
    pub model: Option<String>,
    pub messages: Vec<ExternalMessage>,
    #[serde(default)]
    pub stream: bool,
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
    pub keep_alive: Option<i32>,
    pub min_p: Option<f32>,
    #[serde(alias = "repeat_penalty")]
    pub repetition_penalty: Option<f32>,

    pub temperature: Option<f32>,
    #[serde(alias = "n_predict")]
    pub max_tokens: Option<usize>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub seed: Option<i64>,
    pub response_format: Option<serde_json::Value>,
    pub chat_template_kwargs: Option<serde_json::Value>,
    pub grammar: Option<String>,
    pub logit_bias: Option<std::collections::HashMap<String, f32>>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: Option<serde_json::Value>,
    /// false: installed tools that fit the message are not suggested (default true)
    pub tool_search: Option<bool>,
    /// How the KV cache is matched to the new prompt: "prefix" (only the common prefix is kept),
    /// or "chunk" (llama-server `--cache-reuse`: shared chunks after it are moved, which keeps
    /// their old context). Default: "chunk" only when the Engine dropped old turns to fit.
    #[serde(default)]
    pub kv_reuse: Option<String>,
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

#[derive(Deserialize, Clone)]
pub struct ExternalMessage {
    pub role: String,
    pub content: MessageContent,
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
