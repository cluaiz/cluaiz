use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelSelection {
    pub text: Option<String>,
    pub vision: Option<String>,
    pub audio: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default = "default_temporary_chat_ttl_hours")]
    pub temporary_chat_ttl_hours: u64,
    #[serde(default = "default_enable_kvcache")]
    pub enable_kvcache: bool,
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
            temporary_chat_ttl_hours: default_temporary_chat_ttl_hours(),
            enable_kvcache: default_enable_kvcache(),
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

fn default_temporary_chat_ttl_hours() -> u64 {
    24
}

fn default_enable_kvcache() -> bool {
    true
}

impl PermissionSchema {
    /// Loads the Permission.json from ~/.cluaiz/engine/Permission.json
    /// If it doesn't exist, it creates a default one and returns it.
    pub fn load() -> Self {
        let env_manager = cluaiz_shared::environment::EnvironmentManager::current();
        let config_dir = env_manager.config_dir();
        let permission_path = config_dir.join("Permission.json");
        let permission_bin_path = config_dir.join("Permission.bin");

        // Ensure config dir exists
        let _ = env_manager.ensure_config_dir();

        if !permission_path.exists() {
            warn!("⚠️ Permission.json not found at {:?}. Creating default.", permission_path);
            let default_schema = Self::default();
            if let Err(e) = fs::create_dir_all(&config_dir) {
                warn!("Failed to create config directory: {}", e);
                return default_schema;
            }
            if let Ok(json) = serde_json::to_string_pretty(&default_schema) {
                if let Err(e) = fs::write(&permission_path, json) {
                    warn!("Failed to write default Permission.json: {}", e);
                } else {
                    info!("✅ Created default Permission.json");
                    // Sync to .bin
                    if let Ok(bin_data) = bincode::serialize(&default_schema) {
                        let _ = fs::write(&permission_bin_path, bin_data);
                    }
                }
            }
            return default_schema;
        }

        match fs::read_to_string(&permission_path) {
            Ok(content) => {
                match serde_json::from_str(&content) {
                    Ok(schema) => schema,
                    Err(e) => {
                        warn!("❌ Failed to parse Permission.json: {}. Using default.", e);
                        Self::default()
                    }
                }
            }
            Err(e) => {
                warn!("❌ Failed to read Permission.json: {}. Using default.", e);
                Self::default()
            }
        }
    }

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
        schema.save();
    }

    pub fn set_active_embedding_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.text = Some(model_id);
        schema.save();
    }

    pub fn save(&self) {
        let env_manager = cluaiz_shared::environment::EnvironmentManager::current();
        let config_dir = env_manager.config_dir();
        let permission_path = config_dir.join("Permission.json");
        let permission_bin_path = config_dir.join("Permission.bin");

        let _ = env_manager.ensure_config_dir();
        if let Err(e) = fs::create_dir_all(&config_dir) {
            warn!("Failed to create config directory for saving Permission.json: {}", e);
            return;
        }

        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Err(e) = fs::write(&permission_path, json) {
                warn!("Failed to save Permission.json: {}", e);
            } else {
                info!("✅ Updated Permission.json with active models.");
                // Sync to .bin
                if let Ok(bin_data) = bincode::serialize(self) {
                    let _ = fs::write(&permission_bin_path, bin_data);
                }
            }
        }
    }
}
