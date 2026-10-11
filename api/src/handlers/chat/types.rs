use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::AtomicBool,
    Arc, LazyLock, RwLock,
};

/// Thread-safe multi-stream controller signals
#[derive(Clone, Default)]
pub struct StreamSignals {
    pub cancel: Arc<AtomicBool>,
    pub skip_reasoning: Arc<AtomicBool>,
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
    #[serde(default)]
    pub temporary_chat: Option<TemporaryChatMode>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub think_mode: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
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
    pub async fn flatten_to_string(&self) -> String {
        match self {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Array(parts) => {
                let mut combined = String::new();
                for part in parts {
                    match part {
                        ContentPart::Text { text } => {
                            combined.push_str(text);
                            combined.push('\n');
                        }
                        ContentPart::ImageUrl { image_url } => {
                            let local_path = match crate::url_resolver::resolve_to_local_file(&image_url.url).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve image URL: {}", e);
                                    image_url.url.clone()
                                }
                            };
                            combined.push_str(&format!("<media type=\"image\" url=\"{}\" />\n", local_path));
                        }
                        ContentPart::AudioUrl { audio_url } => {
                            let local_path = match crate::url_resolver::resolve_to_local_file(&audio_url.url).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve audio URL: {}", e);
                                    audio_url.url.clone()
                                }
                            };
                            combined.push_str(&format!("<media type=\"audio\" url=\"{}\" />\n", local_path));
                        }
                        ContentPart::InputAudio { input_audio } => {
                            let data_uri = format!("data:audio/{};base64,{}", input_audio.format, input_audio.data);
                            let local_path = match crate::url_resolver::resolve_to_local_file(&data_uri).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve input_audio: {}", e);
                                    data_uri
                                }
                            };
                            combined.push_str(&format!("<media type=\"audio\" url=\"{}\" />\n", local_path));
                        }
                    }
                }
                combined.trim_end().to_string()
            }
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

/// What the engine really did for the last generation: llama.cpp's own timings (same numbers as
/// llama-cli / llama-server) and the llama.cpp load values of the loaded model.
pub async fn engine_report(host: &cluaiz_runtime::EngineHost) -> Value {
    let plan = host.current_plan.lock().await.clone();
    json!({
        "timings": host.last_timings().await,
        "load": plan.map(|p| json!({
            "placement": format!("{:?}", p.selected_tier),
            "n_ctx": p.context_window,
            "n_gpu_layers": p.n_gpu_layers,
            "n_cpu_moe": p.n_cpu_moe,
            "offload_kqv": p.kv_offload,
            "cache_type": format!("{:?}", p.kv_cache_type),
            "planner": p.explanation,
        })),
    })
}

static CACHED_MODEL_HEADER_INFO: LazyLock<RwLock<Option<(std::time::Instant, Vec<Value>)>>> =
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
        let context_window_total = cluaiz_models::StructuralDNA::parse_context_string(&entry.metadata.context_window);

        // Allocated = what llama.cpp created, only for the model that is loaded (0 otherwise)
        let is_loaded = cluaiz_core::active_model_path()
            .map(|p| !entry.local_dir.is_empty() && std::path::Path::new(&p).starts_with(&entry.local_dir))
            .unwrap_or(false);
        let allocated_ctx = if is_loaded { cluaiz_core::active_context_tokens().unwrap_or(0) } else { 0 };

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
