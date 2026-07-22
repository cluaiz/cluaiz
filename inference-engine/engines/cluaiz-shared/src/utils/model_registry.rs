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
    pub chat_template: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SlotType {
    Chat,
    Embedding,
    Vision,
    Audio,
}

impl SlotType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Vision => "vision",
            Self::Audio => "audio",
        }
    }
}

#[derive(Default)]
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
        }
    }
}

impl SlotType {
    pub fn supported_tasks(&self, caps: &ModelCapabilities) -> Vec<String> {
        match self {
            Self::Chat => crate::utils::model_discovery::rules::chat::get_chat_tasks(caps),
            Self::Embedding => crate::utils::model_discovery::rules::embedding::get_embedding_tasks(caps),
            Self::Vision => crate::utils::model_discovery::rules::vision::get_vision_tasks(caps),
            Self::Audio => crate::utils::model_discovery::rules::audio::get_audio_tasks(caps),
        }
    }

    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        let caps = ModelCapabilities::all_true();
        match category {
            "audio" => SlotType::Audio.supported_tasks(&caps),
            "vision" => SlotType::Vision.supported_tasks(&caps),
            "embedding" => SlotType::Embedding.supported_tasks(&caps),
            _ => SlotType::Chat.supported_tasks(&caps),
        }
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
        crate::environment::EnvironmentManager::current()
            .config_dir()
            .join("model_registry.json")
    }

    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        let caps = ModelCapabilities::all_true();
        match category {
            "audio" => SlotType::Audio.supported_tasks(&caps),
            "vision" => SlotType::Vision.supported_tasks(&caps),
            "embedding" => SlotType::Embedding.supported_tasks(&caps),
            _ => SlotType::Chat.supported_tasks(&caps),
        }
    }

    pub fn load() -> Self {
        let path = Self::get_registry_path();
        if !path.exists() {
            return Self::default();
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
                            if reg.installed_models.contains_key(&id) {
                                continue;
                            }

                            let mut all_files = Vec::new();
                            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                                for f in sub_entries.filter_map(|e| e.ok()) {
                                    let fname = f.file_name().to_string_lossy().to_string();
                                    if fname.ends_with(".gguf")
                                        || fname.ends_with(".onnx")
                                        || fname.ends_with(".bin")
                                    {
                                        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                                        all_files.push((f.path(), fname, size));
                                    }
                                }
                            }

                            if all_files.is_empty() {
                                continue;
                            }

                            all_files.sort_by(|a, b| a.1.cmp(&b.1));

                            let (p_path, p_name, _) = &all_files[0];
                            let p_path_clone = p_path.clone();
                            let p_name_clone = p_name.clone();

                            let format_type = if p_name_clone.ends_with(".gguf") {
                                "gguf"
                            } else {
                                "onnx"
                            };

                            // Delegate Capability Discovery & Metadata probing cleanly to model_discovery module
                            let (slot_type, final_caps, metadata) = crate::utils::model_discovery::CapabilityResolver::discover(
                                &p_path_clone,
                                &entry.path(),
                                cat,
                            );

                            let mut files = Vec::new();
                            for (_fpath, fname, fsize) in &all_files {
                                files.push(RegistryModelFile {
                                    name: fname.clone(),
                                    size_bytes: *fsize,
                                    is_primary: fname == &p_name_clone,
                                });
                            }

                            let registry_entry = ModelRegistryEntry {
                                id: id.clone(),
                                category: slot_type.as_str().to_string(),
                                format_type: format_type.to_string(),
                                huggingface_repo: "".to_string(),
                                local_dir: entry.path().to_string_lossy().to_string(),
                                files,
                                supported_tasks: slot_type.supported_tasks(&final_caps),
                                requires_gpu: false,
                                metadata,
                            };

                            println!("  {} Found new local model folder '{}'. Registering via header read...", "📦".green(), id);
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
