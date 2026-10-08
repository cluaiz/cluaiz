use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SlotConfig {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub format_type: Option<String>,
    #[serde(default)]
    pub supported_tasks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
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

    /// Safely revokes an API token by exact match, 1-based index, or unambiguous masked suffix.
    /// Guards against empty suffix wiping out all tokens and disabling auth.
    pub fn remove_token(&mut self, target: &str) -> Result<String, &'static str> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err("Target token identifier cannot be empty");
        }

        // 1. Numeric 1-based index (e.g. "1", "2")
        if let Ok(idx) = trimmed.parse::<usize>() {
            if idx >= 1 && idx <= self.tokens.len() {
                let removed = self.tokens.remove(idx - 1);
                self.sanitize_tokens();
                return Ok(removed);
            } else {
                return Err("Token index out of range");
            }
        }

        // 2. Exact plaintext match
        if let Some(pos) = self.tokens.iter().position(|t| t.trim() == trimmed) {
            let removed = self.tokens.remove(pos);
            self.sanitize_tokens();
            return Ok(removed);
        }

        // 3. Masked pattern match (e.g. "sk-••••1234" or "••••abcd")
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



#[derive(Debug, Serialize, Deserialize, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PermissionSchema {
    #[serde(default)]
    pub active_slots: std::collections::HashMap<String, SlotConfig>,
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
            active_slots: std::collections::HashMap::new(),
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
        }
    }
}

fn default_agent_security_mode() -> String {
    "sandboxed".to_string()
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
    // Removed custom load method. It is now handled by engine_core::define_config!

    /// Automatically scans installed models and assigns defaults if null
    pub fn auto_assign_defaults(&mut self) {
        // [User Request]: Disabled automatic model assignment.
        // Models will remain null by default until explicitly set by the user via CLI or UI.
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

    pub fn sync_active_slots(&mut self) {
        let roster = crate::models::registry::CoreRoster::load_roster();
        let mut new_slots = std::collections::HashMap::new();

        // Helper to detect properties of a model dynamically by probing weights or using structural DNA
        let get_model_props = |model_id: &str| -> (String, Vec<String>, bool, bool) {
            let clean_id = model_id.replace(":", "-").to_lowercase();
            
            // 1. Try loading from Core Roster manifest to check local path
            let manifest = roster.iter().find(|m| {
                m.id.to_lowercase() == clean_id ||
                m.id.replace(":", "-").to_lowercase() == clean_id ||
                m.huggingface_filename.to_lowercase().contains(&clean_id) ||
                clean_id.contains(&m.huggingface_filename.to_lowercase()) ||
                m.name.to_lowercase() == clean_id
            });

            let mut local_path = None;
            let mut manifest_has_vision = false;
            let mut manifest_has_audio = false;
            let mut format = "gguf".to_string();

            if let Some(m) = manifest {
                local_path = m.local_path.clone().map(std::path::PathBuf::from);
                manifest_has_vision = m.has_vision;
                manifest_has_audio = m.has_audio;
                format = m.architecture_type.clone();
            }

            // Fallback path search if manifest didn't resolve path
            let search_path = local_path.unwrap_or_else(|| {
                engine_core::environment::EnvironmentManager::current()
                    .models_dir()
                    .join(&clean_id)
            });

            // Dynamic format detection based on weights files inside search_path
            if search_path.exists() && search_path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&search_path) {
                    let mut has_gguf = false;
                    let mut has_onnx = false;
                    let mut has_transformer = false;
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                            if ext == "gguf" {
                                has_gguf = true;
                            } else if ext == "onnx" {
                                has_onnx = true;
                            } else if ext == "safetensors" || ext == "bin" || ext == "pt" || path.file_name().and_then(|s| s.to_str()) == Some("config.json") {
                                has_transformer = true;
                            }
                        }
                    }
                    if has_gguf {
                        format = "gguf".to_string();
                    } else if has_onnx {
                        format = "onnx".to_string();
                    } else if has_transformer {
                        format = "Transformer".to_string();
                    }
                }
            }

            let mut has_vision = manifest_has_vision;
            let mut has_audio = manifest_has_audio;
            let mut detected_arch = String::new();

            // 2. Real weights probing (No hardcoded names!)
            if search_path.exists() && search_path.is_dir() {
                // Scan for gguf file to run GGUFProber
                let mut gguf_file = None;
                let mut onnx_file = None;

                if let Ok(entries) = std::fs::read_dir(&search_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                                gguf_file = Some(path);
                                break;
                            } else if path.extension().and_then(|s| s.to_str()) == Some("onnx") {
                                onnx_file = Some(path);
                            }
                        }
                    }
                }

                if let Some(gp) = gguf_file {
                    format = "gguf".to_string();
                    if let Ok((metadata, tensor_infos, _count)) = crate::models::GgufProber::probe(&gp) {
                        if let Some(arch) = metadata.get("general.architecture") {
                            detected_arch = arch.to_lowercase();
                            if detected_arch == "whisper" {
                                has_audio = true;
                            } else if detected_arch == "bert" {
                                // BERT family is for embeddings
                            }
                        }

                        // Dynamic vision layer check: GGUF models with clip vision projection layers
                        let has_clip = tensor_infos.keys().any(|k| k.contains("mm_projector") || k.contains("v_projector") || k.contains("vision"));
                        if has_clip {
                            has_vision = true;
                        }
                    }
                } else if onnx_file.is_some() {
                    format = "onnx".to_string();
                    // Fallback configuration analysis from structural DNA
                    let dna_path = search_path.join("structural_dna.json");
                    if let Ok(dna_str) = std::fs::read_to_string(&dna_path) {
                        if let Ok(dna) = serde_json::from_str::<engine_core::StructuralDNA>(&dna_str) {
                            has_vision = dna.signature.is_multimodal;
                        }
                    }
                }
            }

            // Assign tasks dynamically via UniversalModelClassifier (Single Source of Truth)
            let classification = crate::models::UniversalModelClassifier::classify(
                &clean_id,
                None,
                &[],
                &[],
                if detected_arch.is_empty() { None } else { Some(&detected_arch) },
            );
            let tasks = classification.supported_tasks;
            if classification.capabilities.has_vision {
                has_vision = true;
            }
            if classification.capabilities.is_tts || classification.capabilities.is_asr {
                has_audio = true;
            }

            (format, tasks, has_vision, has_audio)
        };

        // Synchronize active_slots overrides back into primary model fields
        if let Some(slot) = self.active_slots.get("chat_slot") {
            if let Some(ref mid) = slot.model_id {
                if !mid.trim().is_empty() {
                    self.chat_models.text = Some(mid.clone());
                }
            }
        }
        if let Some(slot) = self.active_slots.get("embed_slot") {
            if let Some(ref mid) = slot.model_id {
                if !mid.trim().is_empty() {
                    self.vector_models.text = Some(mid.clone());
                }
            }
        }
        if let Some(slot) = self.active_slots.get("ingest_slot").or_else(|| self.active_slots.get("vision_slot")) {
            if let Some(ref mid) = slot.model_id {
                if !mid.trim().is_empty() {
                    self.vector_models.vision = Some(mid.clone());
                }
            }
        }
        if let Some(slot) = self.active_slots.get("tts_slot") {
            if let Some(ref mid) = slot.model_id {
                if !mid.trim().is_empty() {
                    self.vector_models.audio = Some(mid.clone());
                }
            }
        }
        if let Some(slot) = self.active_slots.get("stt_slot").or_else(|| self.active_slots.get("audio_slot")) {
            if let Some(ref mid) = slot.model_id {
                if !mid.trim().is_empty() {
                    self.vector_models.audio = Some(mid.clone());
                }
            }
        }

        // 1. Process Chat Model
        if let Some(ref chat_id) = self.chat_models.text {
            let (format, tasks, _, _) = get_model_props(chat_id);
            new_slots.insert("chat_slot".to_string(), SlotConfig {
                model_id: Some(chat_id.clone()),
                format_type: Some(format),
                supported_tasks: tasks,
            });
        }

        // 2. Process Embedding Model
        if let Some(ref embed_id) = self.vector_models.text {
            let (format, tasks, _, _) = get_model_props(embed_id);
            new_slots.insert("embed_slot".to_string(), SlotConfig {
                model_id: Some(embed_id.clone()),
                format_type: Some(format),
                supported_tasks: tasks,
            });
        }

        // 3. Process Ingest / Document AI Model
        if let Some(ref ingest_id) = self.vector_models.vision {
            let (format, tasks, _, _) = get_model_props(ingest_id);
            let mut final_tasks = tasks;
            if final_tasks.is_empty() {
                final_tasks = vec!["document-ocr".to_string(), "image-to-text".to_string(), "spatial-vision".to_string()];
            }
            new_slots.insert("ingest_slot".to_string(), SlotConfig {
                model_id: Some(ingest_id.clone()),
                format_type: Some(format.clone()),
                supported_tasks: final_tasks.clone(),
            });
            // Legacy alias
            new_slots.insert("vision_slot".to_string(), SlotConfig {
                model_id: Some(ingest_id.clone()),
                format_type: Some(format),
                supported_tasks: final_tasks,
            });
        }

        // 4. Process Audio Slots (TTS & STT)
        if let Some(ref audio_id) = self.vector_models.audio {
            let (format, tasks, _, _) = get_model_props(audio_id);
            let clean = audio_id.to_lowercase();
            if clean.contains("kokoro") || clean.contains("piper") || clean.contains("tts") {
                new_slots.insert("tts_slot".to_string(), SlotConfig {
                    model_id: Some(audio_id.clone()),
                    format_type: Some(format.clone()),
                    supported_tasks: vec!["text_to_speech".to_string(), "voice-synthesis".to_string()],
                });
            } else {
                new_slots.insert("stt_slot".to_string(), SlotConfig {
                    model_id: Some(audio_id.clone()),
                    format_type: Some(format.clone()),
                    supported_tasks: vec!["speech_to_text".to_string(), "automatic-speech-recognition".to_string()],
                });
            }
            // Legacy alias
            new_slots.insert("audio_slot".to_string(), SlotConfig {
                model_id: Some(audio_id.clone()),
                format_type: Some(format),
                supported_tasks: tasks,
            });
        }

        for (k, v) in new_slots {
            self.active_slots.insert(k, v);
        }
    }

    pub fn set_active_chat_model(model_id: String) {
        let mut schema = Self::load();
        schema.chat_models.text = Some(model_id);
        schema.sync_active_slots();
        let _ = schema.save();
    }

    pub fn set_active_embedding_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.text = Some(model_id);
        schema.sync_active_slots();
        let _ = schema.save();
    }

    pub fn set_active_vision_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.vision = Some(model_id);
        schema.sync_active_slots();
        let _ = schema.save();
    }

    pub fn set_active_audio_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.audio = Some(model_id);
        schema.sync_active_slots();
        let _ = schema.save();
    }

    pub fn merge_patch(&mut self, patch: &serde_json::Value) {
        if let Some(obj) = patch.as_object() {
            if let Some(auth_val) = obj.get("api_auth") {
                let (req_opt, toks_opt) = if let Some(auth_obj) = auth_val.as_object() {
                    let req = auth_obj.get("required").and_then(|v| v.as_bool());
                    let toks = auth_obj.get("tokens").and_then(|v| v.as_array().cloned());
                    (req, toks)
                } else if let Ok(incoming_auth) = serde_json::from_value::<ApiAuth>(auth_val.clone()) {
                    let toks = serde_json::to_value(&incoming_auth.tokens).ok().and_then(|v| v.as_array().cloned());
                    (Some(incoming_auth.required), toks)
                } else {
                    (None, None)
                };

                if let Some(req) = req_opt {
                    self.api_auth.required = req;
                }
                if let Some(toks) = toks_opt {
                    let mut updated_tokens = Vec::new();
                    for t in toks.iter().filter_map(|t| t.as_str().map(|s| s.trim())) {
                        if t.contains('•') || t.contains('*') {
                            let suffix = t.trim_start_matches(|c| c == '•' || c == '*' || c == '-' || c == 's' || c == 'k');
                            if let Some(existing) = self.api_auth.tokens.iter().find(|orig| orig.ends_with(suffix)) {
                                updated_tokens.push(existing.clone());
                            }
                        } else if !t.is_empty() && t != "sk-cluaiz-" && t.len() > 10 {
                            updated_tokens.push(t.to_string());
                        }
                    }
                    self.api_auth.tokens = updated_tokens;
                }
                self.api_auth.ensure_valid_token();
            }

            if let Some(wf) = obj.get("wasm_firewall").and_then(|v| v.as_str()) {
                self.wasm_firewall = wf.to_string();
            }
            if let Some(asm) = obj.get("agent_security_mode").and_then(|v| v.as_str()) {
                self.agent_security_mode = asm.to_string();
            }
            if let Some(v) = obj.get("vectorize_user_input").and_then(|v| v.as_bool()) {
                self.vectorize_user_input = v;
            }
            if let Some(v) = obj.get("vectorize_ai_response").and_then(|v| v.as_bool()) {
                self.vectorize_ai_response = v;
            }
            if let Some(v) = obj.get("stream_telemetry").and_then(|v| v.as_bool()) {
                self.stream_telemetry = v;
            }
            if let Some(v) = obj.get("lazy_load_model").and_then(|v| v.as_bool()) {
                self.lazy_load_model = v;
            }
            if let Some(v) = obj.get("enable_kvcache").and_then(|v| v.as_bool()) {
                self.enable_kvcache = v;
            }
            if let Some(v) = obj.get("model_header_info").and_then(|v| v.as_bool()) {
                self.model_header_info = v;
            }
            if let Some(v) = obj.get("api_port").and_then(|v| v.as_u64()) {
                self.api_port = v as u16;
            }
            if let Some(v) = obj.get("api_host").and_then(|v| v.as_str()) {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    self.api_host = trimmed.to_string();
                }
            }
            if let Some(v) = obj.get("connection_protocol").and_then(|v| v.as_str()) {
                self.connection_protocol = v.to_string();
            }
            if let Some(slots) = obj.get("active_slots") {
                if let Ok(parsed_slots) = serde_json::from_value::<std::collections::HashMap<String, SlotConfig>>(slots.clone()) {
                    self.active_slots = parsed_slots;
                }
            }
            if let Some(cm) = obj.get("chat_models") {
                if let Ok(parsed_cm) = serde_json::from_value::<ModelSelection>(cm.clone()) {
                    self.chat_models = parsed_cm;
                }
            }
            if let Some(vm) = obj.get("vector_models") {
                if let Ok(parsed_vm) = serde_json::from_value::<ModelSelection>(vm.clone()) {
                    self.vector_models = parsed_vm;
                }
            }
        }
        self.sync_active_slots();
    }

    // Removed custom save method. It is now handled by engine_core::define_config!
}

engine_core::define_config!(PermissionSchema, "permission");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_token_exact_and_index() {
        let mut auth = ApiAuth {
            required: true,
            tokens: vec![
                "sk-cluaiz-111111111111".to_string(),
                "sk-cluaiz-222222222222".to_string(),
                "sk-cluaiz-333333333333".to_string(),
            ],
        };

        // Test 1-based index removal
        assert!(auth.remove_token("2").is_ok());
        assert_eq!(auth.tokens.len(), 2);
        assert_eq!(auth.tokens[1], "sk-cluaiz-333333333333");

        // Test exact string match removal
        assert!(auth.remove_token("sk-cluaiz-111111111111").is_ok());
        assert_eq!(auth.tokens.len(), 1);
        assert_eq!(auth.tokens[0], "sk-cluaiz-333333333333");
    }

    #[test]
    fn test_remove_token_masked_suffix_guards() {
        let mut auth = ApiAuth {
            required: true,
            tokens: vec![
                "sk-cluaiz-aaa1234".to_string(),
                "sk-cluaiz-bbb1234".to_string(),
                "sk-cluaiz-ccc9999".to_string(),
            ],
        };

        // Guard 1: Empty or short mask must FAIL, not wipe out all tokens!
        assert!(auth.remove_token("sk-••••••••").is_err());
        assert_eq!(auth.tokens.len(), 3);

        assert!(auth.remove_token("••••1").is_err());
        assert_eq!(auth.tokens.len(), 3);

        // Guard 2: Ambiguous mask matching multiple tokens must FAIL
        assert!(auth.remove_token("sk-••••1234").is_err());
        assert_eq!(auth.tokens.len(), 3);

        // Guard 3: Unambiguous mask matching exactly 1 token succeeds
        assert!(auth.remove_token("sk-••••9999").is_ok());
        assert_eq!(auth.tokens.len(), 2);
        assert!(!auth.tokens.iter().any(|t| t.ends_with("9999")));
    }
}
