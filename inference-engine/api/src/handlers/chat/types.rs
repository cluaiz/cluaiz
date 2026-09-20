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
    // Cluaiz Extension & OpenAI Reasoning Parameters
    pub think_mode: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
    pub skip_reasoning: Option<bool>,
    pub response_length: Option<serde_json::Value>,
    pub keep_alive: Option<i32>,
    pub min_p: Option<f32>,
    pub repetition_penalty: Option<f32>,

    // Standard OpenAI Parameters
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub seed: Option<i64>,
    pub response_format: Option<serde_json::Value>,
    pub logit_bias: Option<std::collections::HashMap<String, f32>>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: Option<serde_json::Value>,
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

/// Helper to fetch real model header and hardware slot allocations
pub fn generate_model_header_info() -> Vec<Value> {
    let registry = cluaiz_shared::hardware::governor::HardwareGovernor::get_active_allocations();
    let mut loaded_models = Vec::new();
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let roots = vec![env.local_dir.join("models"), env.global_dir.join("models")];
    let categories = ["chat", "embedding", "vision", "audio", "code"];

    for info in registry {
        let mut think_start = String::new();
        let mut think_close = String::new();
        let mut all_metadata = json!({});
        let mut context_window_total = info.context_size;

        let mut probed = false;
        
        for root in &roots {
            if probed { break; }
            for category in &categories {
                if probed { break; }
                let cat_dir = root.join(category);
                if let Ok(dirs) = std::fs::read_dir(&cat_dir) {
                    for d in dirs.flatten() {
                        if let Ok(files) = std::fs::read_dir(d.path()) {
                            for f in files.flatten() {
                                let p = f.path();
                                let fname = p.file_name().unwrap_or_default().to_string_lossy();
                                if fname == info.model_id && p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                                    if let Ok((meta, _tensors, _count)) = engines::models::GgufProber::probe(&p) {
                                        let mut meta_map = serde_json::Map::new();
                                        for (k, v) in meta {
                                            meta_map.insert(k.clone(), Value::String(v.clone()));
                                            if k.contains("context_length") {
                                                if let Ok(ctx) = v.parse::<usize>() {
                                                    context_window_total = ctx;
                                                }
                                            }
                                            if k.contains("think_start") || k.contains("thought_start") {
                                                think_start = v.clone();
                                            }
                                            if k.contains("think_close") || k.contains("think_end") || k.contains("thought_end") {
                                                think_close = v.clone();
                                            }
                                        }
                                        all_metadata = Value::Object(meta_map);
                                        probed = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        loaded_models.push(json!({
            "model_id": info.model_id,
            "engine": info.engine,
            "context_window_total": context_window_total,
            "context_window_allocated": info.context_size,
            "think_start_tag": think_start,
            "think_close_tag": think_close,
            "raw_header": all_metadata
        }));
    }
    
    loaded_models
}
