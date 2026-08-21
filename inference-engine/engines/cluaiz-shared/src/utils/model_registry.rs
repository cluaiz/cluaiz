use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryModelFile {
    pub name: String,
    pub size_bytes: u64,
    pub is_primary: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryModelMetadata {
    pub architecture: String,
    pub parameters: String,
    pub context_window: String,
    pub quantization: Option<String>,
    pub bit_depth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_start_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_end_tag: Option<String>,
    pub chat_template: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SlotType {
    Chat,
    Ingest,
    Embedding,
    Tts,
    Stt,
}

impl SlotType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Ingest => "ingest",
            Self::Embedding => "embedding",
            Self::Tts => "tts",
            Self::Stt => "stt",
        }
    }

    pub fn from_category(cat: &str) -> Self {
        let cat_lower = cat.to_lowercase().replace('_', "-");
        match cat_lower.as_str() {
            "chat" | "conversational" | "text-generation" => Self::Chat,
            "ingest" | "vision-ingest" | "ocr" | "doc-ai" => Self::Ingest,
            "embedding" | "embeddings" | "text-embedding" | "vision-embedding" => Self::Embedding,
            "tts" | "text-to-speech" | "audio" => Self::Tts,
            "stt" | "automatic-speech-recognition" | "asr" => Self::Stt,
            _ => Self::Chat,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelCapabilities {
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_video: bool,
    pub has_file: bool,
    pub is_base: bool,
    pub is_instruct: bool,
    pub is_embedding: bool,
    pub is_feature_extraction: bool,
    pub is_vision_chat: bool,
    pub is_image_to_text: bool,
    pub is_vqa: bool,
    pub is_image_gen: bool,
    pub is_video_gen: bool,
    pub is_asr: bool,
    pub is_tts: bool,
    pub is_audio_to_audio: bool,
    pub is_audio_class: bool,
    pub chat_completion: bool,
    pub tts_family: Option<String>,
    pub stt_family: Option<String>,
    pub explicit_tasks: Vec<String>,
}

impl ModelCapabilities {
    pub fn all_true() -> Self {
        Self {
            has_vision: true,
            has_audio: true,
            has_video: true,
            has_file: true,
            is_base: true,
            is_instruct: true,
            is_embedding: true,
            is_feature_extraction: true,
            is_vision_chat: true,
            is_image_to_text: true,
            is_vqa: true,
            is_image_gen: true,
            is_video_gen: true,
            is_asr: true,
            is_tts: true,
            is_audio_to_audio: true,
            is_audio_class: true,
            chat_completion: true,
            tts_family: None,
            stt_family: None,
            explicit_tasks: vec![],
        }
    }
}

impl SlotType {
    pub fn supported_tasks(&self, caps: &ModelCapabilities) -> Vec<String> {
        match self {
            Self::Chat => {
                let mut tasks = vec!["chat-completion".to_string()];
                if caps.has_vision || caps.is_vision_chat {
                    tasks.push("image-text-to-text".to_string());
                    tasks.push("image-to-text".to_string());
                    tasks.push("visual-question-answering".to_string());
                }
                if caps.has_video {
                    tasks.push("video-text-to-text".to_string());
                }
                if caps.has_audio {
                    tasks.push("audio-text-to-text".to_string());
                }
                tasks
            }
            Self::Embedding => {
                vec![
                    "sentence-similarity".to_string(),
                    "feature-extraction".to_string(),
                    "text-classification".to_string(),
                    "zero-shot-image-classification".to_string(),
                    "image-feature-extraction".to_string(),
                    "visual-document-retrieval".to_string(),
                ]
            }
            Self::Ingest => {
                vec![
                    "document-ocr".to_string(),
                    "document-question-answering".to_string(),
                    "table-extraction".to_string(),
                    "object-detection".to_string(),
                    "mask-generation".to_string(),
                ]
            }
            Self::Tts => {
                vec![
                    "text-to-speech".to_string(),
                    "voice-synthesis".to_string(),
                ]
            }
            Self::Stt => {
                vec![
                    "automatic-speech-recognition".to_string(),
                    "audio-classification".to_string(),
                ]
            }
        }
    }

    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        let caps = ModelCapabilities::all_true();
        let slot = Self::from_category(category);
        slot.supported_tasks(&caps)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelRegistryEntry {
    pub id: String,
    pub category: String,
    pub format_type: String,
    pub huggingface_repo: String,
    pub local_dir: String,
    pub files: Vec<RegistryModelFile>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra_files: serde_json::Value,
    pub supported_tasks: Vec<String>,
    pub requires_gpu: bool,
    pub metadata: RegistryModelMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelRegistry {
    pub installed_models: HashMap<String, ModelRegistryEntry>,
}

impl ModelRegistry {
    pub fn get_registry_path() -> PathBuf {
        let primary = crate::environment::EnvironmentManager::current()
            .config_dir()
            .join("model_registry.json");
        if primary.exists() {
            return primary;
        }
        let fallback = PathBuf::from(".cluaiz/engine/config/model_registry.json");
        if fallback.exists() {
            return fallback;
        }
        primary
    }

    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        SlotType::get_tasks_for_category(category)
    }

    pub fn load() -> Self {
        let mut path = Self::get_registry_path();
        if !path.exists() {
            let fallback = PathBuf::from(".cluaiz/engine/config/model_registry.json");
            if fallback.exists() {
                path = fallback;
            } else {
                return Self::default();
            }
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        let mut reg: ModelRegistry = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => return Self::default(),
        };

        let mut to_remove = Vec::new();
        for (id, entry) in &reg.installed_models {
            let dir_path = Path::new(&entry.local_dir);

            if !dir_path.exists() {
                to_remove.push(id.clone());
                continue;
            }

            if let Some(primary) = entry.files.iter().find(|f| f.is_primary) {
                let primary_file_path = dir_path.join(&primary.name);
                if !primary_file_path.exists() {
                    to_remove.push(id.clone());
                }
            }
        }

        if !to_remove.is_empty() {
            let mut changed = false;
            for id in to_remove {
                reg.installed_models.remove(&id);
                changed = true;
            }
            if changed {
                let _ = reg.save();
            }
        }

        reg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::get_registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&path, content).map_err(|e| format!("Write error: {}", e))?;

        Ok(())
    }

    pub fn sync_from_disk(base_models_dir: &std::path::Path) {
        use color_eyre::owo_colors::OwoColorize;

        println!(
            "  {} [Cluaiz] Boot-time dynamic model scan initiated...",
            "📡".cyan()
        );

        let mut reg = Self::load();
        let mut changes_made = false;

        let categories = ["chat", "audio", "vision", "embedding"];
        for cat in &categories {
            let cat_dir = base_models_dir.join(cat);
            if !cat_dir.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(&cat_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let id = entry.file_name().to_string_lossy().to_string();

                            let mut all_weight_files = Vec::new();
                            let mut extra_files_list = Vec::new();

                            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                                for f in sub_entries.filter_map(|e| e.ok()) {
                                    let fname = f.file_name().to_string_lossy().to_string();
                                    if !fname.starts_with('.') && fname != "cluaiz-engine.ready" {
                                        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                                        let fpath = f.path();

                                        if fpath.is_dir() {
                                            let mut subfolder_files = Vec::new();
                                            if let Ok(deep_entries) = std::fs::read_dir(&fpath) {
                                                for df in deep_entries.filter_map(|e| e.ok()) {
                                                    let dfname = df.file_name().to_string_lossy().to_string();
                                                    subfolder_files.push(serde_json::Value::String(dfname));
                                                }
                                            }
                                            let mut sub_map = serde_json::Map::new();
                                            sub_map.insert(fname, serde_json::Value::Array(subfolder_files));
                                            extra_files_list.push(serde_json::Value::Object(sub_map));
                                        } else if fname.ends_with(".gguf") || fname.ends_with(".onnx") {
                                            all_weight_files.push((fpath, fname, size));
                                        } else {
                                            extra_files_list.push(serde_json::Value::String(fname));
                                        }
                                    }
                                }
                            }
                            let extra_files = serde_json::Value::Array(extra_files_list);

                            if cat == &"audio" && all_weight_files.len() > 1 {
                                all_weight_files.sort_by(|a, b| {
                                    let name_a = a.1.to_lowercase();
                                    let name_b = b.1.to_lowercase();
                                    let score_a = if name_a.contains("flow") || name_a.contains("estimator") || name_a.contains("generator") || name_a.contains("tts") || name_a.contains("synth") {
                                        0
                                    } else if name_a.contains("campplus") || name_a.contains("speaker") || name_a.contains("embed") || name_a.contains("encoder") {
                                        2
                                    } else {
                                        1
                                    };
                                    let score_b = if name_b.contains("flow") || name_b.contains("estimator") || name_b.contains("generator") || name_b.contains("tts") || name_b.contains("synth") {
                                        0
                                    } else if name_b.contains("campplus") || name_b.contains("speaker") || name_b.contains("embed") || name_b.contains("encoder") {
                                        2
                                    } else {
                                        1
                                    };
                                    score_a.cmp(&score_b).then_with(|| a.1.cmp(&b.1))
                                });
                            }

                            if all_weight_files.is_empty() {
                                continue;
                            }

                            let (p_path, p_name, _) = &all_weight_files[0];
                            let p_path_clone = p_path.clone();
                            let p_name_clone = p_name.clone();

                            let format_type = if p_name_clone.ends_with(".gguf") {
                                "gguf"
                            } else {
                                "onnx"
                            };

                            // Delegate Capability Discovery & Metadata probing cleanly to model_discovery module
                            let (slot_type, final_caps, mut metadata, requires_gpu) =
                                crate::utils::model_discovery::CapabilityResolver::discover(
                                    &p_path_clone,
                                    &entry.path(),
                                    cat,
                                );

                            // Also check local model_manifest.json if present for human parameters override
                            let manifest_file = entry.path().join("model_manifest.json");
                            if manifest_file.exists() {
                                if let Ok(manifest_content) =
                                    std::fs::read_to_string(&manifest_file)
                                {
                                    if let Ok(manifest_val) =
                                        serde_json::from_str::<serde_json::Value>(&manifest_content)
                                    {
                                        if let Some(param_str) =
                                            manifest_val.get("parameters").and_then(|p| p.as_str())
                                        {
                                            if !param_str.trim().is_empty()
                                                && param_str != "Unknown"
                                            {
                                                metadata.parameters = param_str.to_string();
                                            }
                                        }
                                    }
                                }
                            }



                            let mut files = Vec::new();
                            for (_fpath, fname, fsize) in &all_weight_files {
                                files.push(RegistryModelFile {
                                    name: fname.clone(),
                                    size_bytes: *fsize,
                                    is_primary: fname == &p_name_clone,
                                });
                            }

                            let hf_repo = reg
                                .installed_models
                                .get(&id)
                                .map(|e| e.huggingface_repo.clone())
                                .unwrap_or_default();

                            let registry_entry = ModelRegistryEntry {
                                id: id.clone(),
                                category: slot_type.as_str().to_string(),
                                format_type: format_type.to_string(),
                                huggingface_repo: hf_repo,
                                local_dir: entry.path().to_string_lossy().to_string(),
                                files,
                                extra_files,
                                supported_tasks: slot_type.supported_tasks(&final_caps),
                                requires_gpu,
                                metadata,
                            };

                            if !reg.installed_models.contains_key(&id) {
                                println!("  {} Found new local model folder '{}'. Registering via header read...", "📦".green(), id);
                            }
                            reg.installed_models.insert(id, registry_entry);
                            changes_made = true;
                        }
                    }
                }
            }
        }

        if changes_made {
            if let Err(e) = reg.save() {
                println!(
                    "  {} Failed to save model registry after boot scan: {}",
                    "⚠️".yellow(),
                    e
                );
            } else {
                println!(
                    "  {} Model registry synchronized dynamically successfully.",
                    "✅".green()
                );
            }
        }
    }

    /// Atomically insert or update a model registration entry
    pub fn register_model(entry: ModelRegistryEntry) -> Result<(), String> {
        let mut reg = Self::load();
        reg.installed_models.insert(entry.id.clone(), entry);
        reg.save()
    }

    /// Remove a model from the registry mapping
    pub fn unregister_model(id: &str) -> Result<(), String> {
        let mut reg = Self::load();
        if reg.installed_models.remove(id).is_some() {
            reg.save()?;
        }
        Ok(())
    }
}
