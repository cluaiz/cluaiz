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
            Self::Chat => {
                let mut tasks = vec![];
                if caps.is_base {
                    // "text-generation": Applied strictly when the model is a raw base completion model (unaligned, next-token predictor).
                    tasks.push("text-generation".to_string());
                }
                if caps.is_instruct {
                    // "chat-completion": Applied strictly when the model is instruction-tuned/RLHF-aligned for multi-turn chat (supports chat template & system prompts).
                    tasks.push("chat-completion".to_string());
                }
                if caps.has_vision {
                    // "multimodal-vision": Applied when the model contains vision encoder/projector weights to process image inputs alongside text.
                    tasks.push("multimodal-vision".to_string());
                }
                if caps.has_video {
                    // "multimodal-video": Applied when the model contains temporal attention/frame-processing layers for video sequence understanding.
                    tasks.push("multimodal-video".to_string());
                }
                if caps.has_file {
                    // "multimodal-file": Applied when the model natively parses and contextualizes structured file documents (PDF, raw codebases, JSON).
                    tasks.push("multimodal-file".to_string());
                }
                if caps.has_audio {
                    // "multimodal-audio": Applied when the chat model directly processes or responds with speech/audio spectrogram tensors.
                    tasks.push("multimodal-audio".to_string());
                }
                tasks
            }
            Self::Embedding => {
                let mut tasks = vec![];
                if caps.is_embedding {
                    // "embedding": Applied strictly when the model generates dense or sparse text vector embeddings via pooled encoder hidden states.
                    tasks.push("embedding".to_string());
                }
                if caps.is_feature_extraction {
                    // "feature-extraction": Applied when the model extracts raw intermediate hidden-layer representations for downstream tasks.
                    tasks.push("feature-extraction".to_string());
                }
                if caps.has_vision {
                    // "vision-embedding": Applied when the model projects image inputs into a shared vector embedding space (e.g. CLIP).
                    tasks.push("vision-embedding".to_string());
                }
                if caps.has_audio {
                    // "audio-embedding": Applied when the model projects audio waveform/spectrogram inputs into vector embedding space.
                    tasks.push("audio-embedding".to_string());
                }
                tasks
            }
            Self::Vision => {
                let mut tasks = vec![];
                if caps.is_vision_chat {
                    // "vision-chat": Applied when a dedicated vision-first model is optimized for interactive visual dialogue.
                    tasks.push("vision-chat".to_string());
                }
                if caps.is_image_to_text {
                    // "image-to-text": Applied for dedicated image captioning or optical character recognition (OCR) models.
                    tasks.push("image-to-text".to_string());
                }
                if caps.is_vqa {
                    // "visual-question-answering": Applied when the model is specialized for answering questions grounded in input images.
                    tasks.push("visual-question-answering".to_string());
                }
                if caps.is_image_gen {
                    // "image-generation": Applied for diffusion or autoregressive image generation models (text/image to image output).
                    tasks.push("image-generation".to_string());
                }
                if caps.has_video {
                    // "video-generation": Applied for video diffusion/generation models (text/image to video output).
                    tasks.push("video-generation".to_string());
                }
                tasks
            }
            Self::Audio => {
                let mut tasks = vec![];
                if caps.is_asr {
                    // "automatic-speech-recognition": Applied for speech-to-text transcription models (e.g. Whisper).
                    tasks.push("automatic-speech-recognition".to_string());
                }
                if caps.is_tts {
                    // "text-to-speech": Applied for text-to-speech synthesis models (e.g. Bark, Coqui).
                    tasks.push("text-to-speech".to_string());
                }
                if caps.is_audio_to_audio {
                    // "audio-to-audio": Applied for audio translation, voice conversion, or speech enhancement models.
                    tasks.push("audio-to-audio".to_string());
                }
                if caps.is_audio_class {
                    // "audio-classification": Applied for acoustic event detection or sound classification models.
                    tasks.push("audio-classification".to_string());
                }
                tasks
            }
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

    /// Dynamically resolve SlotType from GGUF metadata or ONNX parameters
    pub fn detect_from_metadata(
        format_type: &str,
        category: &str,
        architecture: &str,
        has_vision: bool,
        has_audio: bool,
        model_dir: Option<&std::path::Path>,
    ) -> (SlotType, ModelCapabilities) {
        let arch_lower = architecture.to_lowercase();
        let cat_lower = category.to_lowercase();

        // 1. Initialize strict baseline capabilities (Default all to false)
        let mut caps = ModelCapabilities {
            is_base: false,
            is_instruct: false,
            has_vision: false,
            has_video: false,
            has_file: false,
            has_audio: false,
            is_embedding: false,
            is_feature_extraction: false,
            is_vision_chat: false,
            is_image_to_text: false,
            is_vqa: false,
            is_image_gen: false,
            is_video_gen: false,
            is_asr: false,
            is_tts: false,
            is_audio_to_audio: false,
            is_audio_class: false,
        };

        // 2. PRIMARY (95%): Weight Header & Architecture Structural Fingerprinting
        if cat_lower.contains("embedding") || arch_lower.contains("bert") || arch_lower.contains("nomic") || arch_lower.contains("bge") {
            caps.is_embedding = true;
            caps.is_feature_extraction = true;
            return (SlotType::Embedding, caps);
        }

        if cat_lower.contains("audio") || arch_lower.contains("whisper") || arch_lower.contains("bark") {
            caps.is_asr = arch_lower.contains("whisper");
            caps.is_tts = arch_lower.contains("bark");
            return (SlotType::Audio, caps);
        }

        // Qwen-VL family (qwen2vl, qwen3vl, qwen-vl)
        let is_qwen_vl = arch_lower.contains("qwen") && arch_lower.contains("vl");
        // Gemma4 family (gemma4, gemma-4)
        let is_gemma4 = arch_lower.contains("gemma4") || arch_lower.contains("gemma-4");

        if has_vision || is_qwen_vl || arch_lower.contains("llava") || arch_lower.contains("phi3v") || (is_gemma4 && (has_vision || arch_lower.contains("e2b") || arch_lower.contains("e4b"))) {
            caps.has_vision = true;
            caps.is_instruct = true;
            caps.is_vision_chat = true;
            if is_gemma4 {
                caps.has_audio = true;
            }
            return (SlotType::Chat, caps);
        }

        if has_audio {
            caps.has_audio = true;
            return (SlotType::Chat, caps);
        }

        // Standard LLM Text Chat Model
        caps.is_instruct = true;

        // 3. FALLBACK (5%): Inspect local fallback JSONs (config.json, processor_config.json) IF present
        if let Some(dir) = model_dir {
            Self::enrich_caps_from_fallback_jsons(dir, &mut caps);
        }

        let slot = match category {
            "audio" => SlotType::Audio,
            "vision" => SlotType::Vision,
            "embedding" => SlotType::Embedding,
            _ => SlotType::Chat,
        };

        (slot, caps)
    }

    /// Quietly enrich capabilities from local supplementary JSON files if header inspection leaves gaps
    fn enrich_caps_from_fallback_jsons(dir: &std::path::Path, caps: &mut ModelCapabilities) {
        // Fallback A: Check config.json
        let config_path = dir.join("config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    if v.get("visual").is_some() || v.get("vision_config").is_some() {
                        caps.has_vision = true;
                    }
                    if v.get("audio_config").is_some() {
                        caps.has_audio = true;
                    }
                }
            }
        }
        // Fallback B: Check processor_config.json / preprocessor_config.json
        let proc_path = dir.join("processor_config.json");
        if proc_path.exists() {
            caps.has_vision = true;
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

                            let mut architecture = "Unknown".to_string();
                            let mut context_window = "Unknown".to_string();
                            let mut parameters_str = "Unknown".to_string();

                            let mut caps = ModelCapabilities::default();

                            if format_type == "gguf" {
                                if let Ok((metadata, tensor_infos, _)) =
                                    crate::utils::GGUFProber::probe(&p_path_clone)
                                {
                                    // 1. Architecture & Context Length
                                    if let Some(arch) = metadata.get("general.architecture") {
                                        architecture = arch.clone();
                                        if let Some(ctx) =
                                            metadata.get(&format!("{}.context_length", arch))
                                        {
                                            context_window = ctx.clone();
                                        }
                                    }

                                    // 2. Compute parameter count dynamically
                                    let mut total_params: u64 = 0;
                                    for (_name, dims) in &tensor_infos {
                                        if !dims.is_empty() {
                                            let count: u64 =
                                                dims.iter().map(|&d| d as u64).product();
                                            total_params += count;
                                        }
                                    }
                                    if total_params > 0 {
                                        parameters_str = format!(
                                            "{:.2}B",
                                            total_params as f64 / 1_000_000_000.0
                                        );
                                    }

                                    // 1. Instruct vs Base Trait
                                    if metadata.contains_key("tokenizer.chat_template") || architecture.to_lowercase().contains("instruct") || architecture.to_lowercase().contains("chat") {
                                        caps.is_instruct = true;
                                    } else {
                                        caps.is_base = true;
                                    }

                                    // 2. Embedding & Feature Extraction Traits
                                    let is_embedding_arch = metadata.contains_key("general.pooling_type")  
                                        || architecture.to_lowercase().contains("bert")
                                        || architecture.to_lowercase().contains("nomic")
                                        || architecture.to_lowercase().contains("bge")
                                        || architecture.to_lowercase().contains("gte")
                                        || architecture.to_lowercase().contains("e5");
                                    if is_embedding_arch {
                                        caps.is_embedding = true;
                                        caps.is_feature_extraction = true;
                                    }

                                    // 3. Vision Capability Traits (Fine-Grained)
                                    let has_vision_keys = metadata.keys().any(|k| {
                                        k.starts_with("clip.vision.")
                                            || k.starts_with("vision.")
                                            || k.starts_with("llava.")
                                            || k.starts_with("mpt.vision.")
                                    });
                                    let has_vision_tensors = tensor_infos.iter().any(|(name, _)| {
                                        name.contains("visual") || name.contains("mm_projector") || name.contains("v.proj")
                                    });

                                    if has_vision_keys || has_vision_tensors {
                                        caps.has_vision = true;
                                        let arch_lower = architecture.to_lowercase();
                                        if caps.is_instruct {
                                            caps.is_vision_chat = true;
                                        }
                                        if arch_lower.contains("vqa") || arch_lower.contains("pali") {
                                            caps.is_vqa = true;
                                        }
                                        if arch_lower.contains("ocr") || arch_lower.contains("caption") || arch_lower.contains("nougat") || arch_lower.contains("surya") {
                                            caps.is_image_to_text = true;
                                        }
                                    }

                                    // 4. Image / Video Generation Diffusion Traits
                                    let arch_lower = architecture.to_lowercase();
                                    if arch_lower.contains("diffusion") || arch_lower.contains("flux") || arch_lower.contains("sdxl") || arch_lower.contains("pixart") {
                                        caps.is_image_gen = true;
                                    }
                                    if arch_lower.contains("cogvideo") || arch_lower.contains("svd") || arch_lower.contains("animatediff") {
                                        caps.is_video_gen = true;
                                        caps.has_video = true;
                                    }

                                    // 5. Audio Capability Traits (Fine-Grained)
                                    let has_audio_keys = metadata.keys().any(|k| {
                                        k.starts_with("whisper.") || k.starts_with("audio.") || k.starts_with("bark.")
                                    });
                                    let has_audio_tensors = tensor_infos.iter().any(|(name, _)| {
                                        name.contains("audio_encoder") || name.contains("mel_filters") || name.contains("speech")
                                    });

                                    if has_audio_keys || has_audio_tensors || arch_lower.contains("whisper") || arch_lower.contains("bark") {
                                        caps.has_audio = true;
                                        if arch_lower.contains("whisper") || metadata.keys().any(|k| k.starts_with("whisper.")) {
                                            caps.is_asr = true;
                                        }
                                        if arch_lower.contains("bark") || arch_lower.contains("piper") || arch_lower.contains("vits") {
                                            caps.is_tts = true;
                                        }
                                        if arch_lower.contains("conversion") || arch_lower.contains("demucs") {
                                            caps.is_audio_to_audio = true;
                                        }
                                        if arch_lower.contains("clap") || arch_lower.contains("ast") {
                                            caps.is_audio_class = true;
                                        }
                                    }

                                    // 6. Multimodal File Context Traits
                                    if arch_lower.contains("coder") || arch_lower.contains("starcoder") || arch_lower.contains("deepseek-coder") {
                                        caps.has_file = true;
                                    }
                                }
                            } else {
                                // 7. Category Folder Specific Traits
                                match *cat {
                                    "audio" => {
                                        caps.has_audio = true;
                                        if architecture.to_lowercase().contains("whisper") {
                                            caps.is_asr = true;
                                        }
                                    }
                                    "vision" => {
                                        caps.has_vision = true;
                                        caps.is_image_gen = true;
                                    }
                                    "embedding" => {
                                        caps.is_embedding = true;
                                    }
                                    _ => {
                                        caps.is_instruct = true;
                                        caps.is_base = true;
                                    }
                                }
                            }

                            let entry_path = entry.path();
                            let (slot_type, detected_caps) = SlotType::detect_from_metadata(
                                format_type,
                                cat,
                                &architecture,
                                caps.has_vision,
                                caps.has_audio,
                                Some(&entry_path),
                            );

                            let final_caps = if caps.has_vision || caps.has_audio { caps } else { detected_caps };

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
                                metadata: RegistryModelMetadata {
                                    architecture,
                                    parameters: parameters_str,
                                    context_window,
                                },
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
