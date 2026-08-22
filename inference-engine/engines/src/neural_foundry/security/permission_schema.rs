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
                cluaiz_shared::environment::EnvironmentManager::current()
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
                    if let Ok((metadata, tensor_infos, _count)) = cluaiz_shared::utils::GGUFProber::probe(&gp) {
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
                        if let Ok(dna) = serde_json::from_str::<cluaiz_shared::StructuralDNA>(&dna_str) {
                            has_vision = dna.signature.is_multimodal;
                        }
                    }
                }
            }

            // Assign tasks based on dynamic architecture categories (e.g. whisper, piper, kokoro, bert)
            let mut tasks = Vec::new();
            if detected_arch == "whisper" || clean_id.contains("whisper") || clean_id.contains("moonshine") || clean_id.contains("sensevoice") {
                tasks.push("speech_to_text".to_string());
                tasks.push("automatic-speech-recognition".to_string());
            } else if clean_id.contains("kokoro") || clean_id.contains("piper") || clean_id.contains("melotts") || clean_id.contains("tts") {
                tasks.push("text_to_speech".to_string());
                tasks.push("voice-synthesis".to_string());
            } else if has_audio {
                tasks.push("speech_to_text".to_string());
            } else if detected_arch == "bert" || format == "safetensors" || clean_id.contains("embedding") || clean_id.contains("embed") {
                tasks.push("sentence-similarity".to_string());
                tasks.push("feature-extraction".to_string());
                tasks.push("embedding".to_string());
                if has_vision {
                    tasks.push("vision-embedding".to_string());
                }
            } else if clean_id.contains("got-ocr") || clean_id.contains("nougat") || clean_id.contains("florence") || clean_id.contains("table") {
                tasks.push("document-ocr".to_string());
                tasks.push("table-extraction".to_string());
                tasks.push("spatial-vision".to_string());
            } else {
                tasks.push("text-generation".to_string());
                tasks.push("chat-completion".to_string());
                if has_vision {
                    tasks.push("multimodal-vision".to_string());
                }
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

    // Removed custom save method. It is now handled by cluaiz_shared::define_config!
}

cluaiz_shared::define_config!(PermissionSchema, "permission");
