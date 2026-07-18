use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ModelSelection {
    pub text: Option<String>,
    pub vision: Option<String>,
    pub audio: Option<String>,
}

fn default_connection_protocol() -> String {
    "http".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ApiAuth {
    #[serde(default = "default_require_api_auth")]
    pub required: bool,
    #[serde(default = "default_api_tokens")]
    pub tokens: Vec<String>,
}

impl Default for ApiAuth {
    fn default() -> Self {
        Self {
            required: default_require_api_auth(),
            tokens: default_api_tokens(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PermissionSchema {
    #[serde(default)]
    pub vector_models: ModelSelection,
    #[serde(default)]
    pub chat_models: ModelSelection,
    #[serde(default = "default_wasm_firewall")]
    pub wasm_firewall: String,
    #[serde(default = "default_vectorize_user_input")]
    pub vectorize_user_input: bool,
    #[serde(default = "default_vectorize_ai_response")]
    pub vectorize_ai_response: bool,
    #[serde(default = "default_stream_telemetry")]
    pub stream_telemetry: bool,
    #[serde(default = "default_lazy_load_model")]
    pub lazy_load_model: bool,
    #[serde(default = "default_enable_kvcache")]
    pub enable_kvcache: bool,
    #[serde(default = "default_model_header_info")]
    pub model_header_info: bool,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_connection_protocol")]
    pub connection_protocol: String,
    #[serde(default)]
    pub api_auth: ApiAuth,
}

impl Default for ModelSelection {
    fn default() -> Self {
        Self {
            text: None,
            vision: None,
            audio: None,
        }
    }
}

impl Default for PermissionSchema {
    fn default() -> Self {
        Self {
            vector_models: ModelSelection::default(),
            chat_models: ModelSelection::default(),
            wasm_firewall: default_wasm_firewall(),
            vectorize_user_input: default_vectorize_user_input(),
            vectorize_ai_response: default_vectorize_ai_response(),
            stream_telemetry: default_stream_telemetry(),
            lazy_load_model: default_lazy_load_model(),
            enable_kvcache: default_enable_kvcache(),
            model_header_info: default_model_header_info(),
            api_port: default_api_port(),
            connection_protocol: default_connection_protocol(),
            api_auth: ApiAuth::default(),
        }
    }
}

fn default_wasm_firewall() -> String {
    "auto".to_string()
}

fn default_vectorize_user_input() -> bool {
    true
}

fn default_vectorize_ai_response() -> bool {
    true
}

fn default_stream_telemetry() -> bool {
    false
}

fn default_lazy_load_model() -> bool {
    true
}

fn default_enable_kvcache() -> bool {
    true
}

fn default_model_header_info() -> bool {
    false
}

fn default_require_api_auth() -> bool {
    false
}

fn default_api_tokens() -> Vec<String> {
    Vec::new()
}

fn default_api_port() -> u16 {
    8000
}

impl PermissionSchema {
    // Removed custom load method. It is now handled by cluaiz_shared::define_config!

    /// Automatically scans installed models and assigns defaults if null
    pub fn auto_assign_defaults(&mut self) {
        // [User Request]: Disabled automatic model assignment.
        // Models will remain null by default until explicitly set by the user via CLI or UI.
    }

    pub fn get_active_chat_model(&self) -> Option<String> {
        self.chat_models.text.clone()
    }
    pub fn get_active_embedding_model(&self) -> Option<String> {
        self.vector_models.text.clone()
    }

    pub fn set_active_chat_model(model_id: String) {
        let mut schema = Self::load();
        schema.chat_models.text = Some(model_id);
        let _ = schema.save();
    }

    pub fn set_active_embedding_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.text = Some(model_id);
        let _ = schema.save();
    }

    // Removed custom save method. It is now handled by cluaiz_shared::define_config!
}

cluaiz_shared::define_config!(PermissionSchema, "permission");
