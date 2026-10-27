//! 🛡️ Master Permission Schema & Dynamic Hardware Capability Registry (`security/src/permission_schema.rs`)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use cluaiz_core::EnvironmentManager;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SlotConfig {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub format_type: Option<String>,
    #[serde(default)]
    pub supported_tasks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelSelection {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub vision: Option<String>,
    #[serde(default)]
    pub audio: Option<String>,
}

fn default_connection_protocol() -> String {
    "http".to_string()
}

fn default_api_host() -> String {
    "0.0.0.0".to_string()
}

/// What CEP `system.exec` may do. Both off by default: every call asks the owner, and only a
/// program with its arguments runs (no shell line).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExecPolicy {
    /// Accept `{"command": "<shell line>"}` (cmd /C, sh -c) besides `{"program", "args"}`
    #[serde(default)]
    pub allow_shell: bool,
    /// Run without the owner's approval of each call
    #[serde(default)]
    pub without_approval: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

impl ApiAuth {
    pub fn generate_token() -> String {
        format!("sk-cluaiz-{}", uuid::Uuid::new_v4().simple())
    }

    pub fn sanitize_tokens(&mut self) {
        self.tokens.retain(|t| {
            let trimmed = t.trim();
            !trimmed.is_empty() && trimmed != "sk-cluaiz-" && trimmed.len() > 10
        });
    }

    pub fn mask_token(token: &str) -> String {
        let trimmed = token.trim();
        if trimmed.len() <= 8 {
            "sk-••••••••".to_string()
        } else {
            let suffix = &trimmed[trimmed.len().saturating_sub(4)..];
            format!("sk-••••{}", suffix)
        }
    }

    pub fn masked_tokens(&self) -> Vec<String> {
        self.tokens.iter().map(|t| Self::mask_token(t)).collect()
    }

    pub fn ensure_valid_token(&mut self) -> Option<String> {
        self.sanitize_tokens();
        if self.required && self.tokens.is_empty() {
            let new_tok = Self::generate_token();
            self.tokens.push(new_tok.clone());
            Some(new_tok)
        } else {
            None
        }
    }

    pub fn remove_token(&mut self, target: &str) -> Result<String, &'static str> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err("Target token identifier cannot be empty");
        }

        if let Ok(idx) = trimmed.parse::<usize>() {
            if idx >= 1 && idx <= self.tokens.len() {
                let removed = self.tokens.remove(idx - 1);
                self.sanitize_tokens();
                return Ok(removed);
            } else {
                return Err("Token index out of range");
            }
        }

        if let Some(pos) = self.tokens.iter().position(|t| t.trim() == trimmed) {
            let removed = self.tokens.remove(pos);
            self.sanitize_tokens();
            return Ok(removed);
        }

        if trimmed.contains('•') || trimmed.contains('*') {
            if let Some((idx, ch)) = trimmed.char_indices().filter(|(_, c)| *c == '•' || *c == '*').last() {
                let suffix = &trimmed[idx + ch.len_utf8()..];
                if suffix.len() < 4 {
                    return Err("Masked token must have at least 4 visible trailing characters");
                }
                let matching: Vec<usize> = self.tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.ends_with(suffix))
                    .map(|(i, _)| i)
                    .collect();

                if matching.is_empty() {
                    return Err("No token matches the provided masked suffix");
                }
                if matching.len() > 1 {
                    return Err("Multiple tokens match this masked suffix. Use index or full token instead.");
                }
                let removed = self.tokens.remove(matching[0]);
                self.sanitize_tokens();
                return Ok(removed);
            }
        }

        Err("Token not found")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PermissionSchema {
    #[serde(default)]
    pub active_slots: HashMap<String, SlotConfig>,
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
    #[serde(default = "default_api_host")]
    pub api_host: String,
    #[serde(default = "default_connection_protocol")]
    pub connection_protocol: String,
    #[serde(default)]
    pub api_auth: ApiAuth,
    #[serde(default = "default_agent_security_mode")]
    pub agent_security_mode: String,
    #[serde(default)]
    pub exec: ExecPolicy,
}

impl Default for PermissionSchema {
    fn default() -> Self {
        Self {
            active_slots: HashMap::new(),
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
            api_host: default_api_host(),
            connection_protocol: default_connection_protocol(),
            api_auth: ApiAuth::default(),
            agent_security_mode: default_agent_security_mode(),
            exec: ExecPolicy::default(),
        }
    }
}

fn default_wasm_firewall() -> String { "auto".to_string() }
fn default_vectorize_user_input() -> bool { true }
fn default_vectorize_ai_response() -> bool { true }
fn default_stream_telemetry() -> bool { false }
fn default_lazy_load_model() -> bool { true }
fn default_enable_kvcache() -> bool { true }
fn default_model_header_info() -> bool { false }
fn default_require_api_auth() -> bool { false }
fn default_api_tokens() -> Vec<String> { Vec::new() }
fn default_api_port() -> u16 { 8000 }
fn default_agent_security_mode() -> String { "sandboxed".to_string() }

impl PermissionSchema {
    fn file_path() -> PathBuf {
        let env = EnvironmentManager::current();
        let _ = env.ensure_config_dir();
        env.config_dir().join("permission.json")
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(schema) = serde_json::from_str::<Self>(&data) {
                    return schema;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::file_path();
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn get_active_chat_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("chat_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.chat_models.text.clone()
    }

    pub fn get_active_embedding_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("embed_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.text.clone()
    }

    pub fn get_active_vision_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("vision_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.vision.clone()
    }

    pub fn get_active_audio_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("audio_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.audio.clone()
    }

    pub fn set_active_chat_model(model_id: String) {
        let mut schema = Self::load();
        schema.chat_models.text = Some(model_id.clone());
        let slot = schema.active_slots.entry("chat_slot".to_string()).or_default();
        slot.model_id = Some(model_id);
        let _ = schema.save();
    }

    pub fn set_active_embedding_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.text = Some(model_id.clone());
        let slot = schema.active_slots.entry("embed_slot".to_string()).or_default();
        slot.model_id = Some(model_id);
        let _ = schema.save();
    }

    pub fn merge_patch(&mut self, patch: &serde_json::Value) {
        if let Some(obj) = patch.as_object() {
            if let Some(auth_val) = obj.get("api_auth") {
                if let Ok(incoming) = serde_json::from_value::<ApiAuth>(auth_val.clone()) {
                    self.api_auth = incoming;
                }
            }
            if let Some(slots) = obj.get("active_slots") {
                if let Ok(parsed) = serde_json::from_value::<HashMap<String, SlotConfig>>(slots.clone()) {
                    self.active_slots = parsed;
                }
            }
            if let Some(cm) = obj.get("chat_models") {
                if let Ok(parsed) = serde_json::from_value::<ModelSelection>(cm.clone()) {
                    self.chat_models = parsed;
                }
            }
            if let Some(exec) = obj.get("exec") {
                if let Ok(parsed) = serde_json::from_value::<ExecPolicy>(exec.clone()) {
                    self.exec = parsed;
                }
            }
            if let Some(vm) = obj.get("vector_models") {
                if let Ok(parsed) = serde_json::from_value::<ModelSelection>(vm.clone()) {
                    self.vector_models = parsed;
                }
            }
        }
    }
}
