//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: Deep Binary Inspection & Autonomous Capability Discovery (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

pub mod gguf;
pub mod onnx;
pub mod fallback;

pub use gguf::{GgufProbeResult, GgufProber};
pub use onnx::{OnnxProbeResult, OnnxProber};
pub use fallback::{FallbackJsonMetadata, FallbackProber};

use std::collections::HashMap;
use std::path::Path;
use crate::models::types::entities::SlotType;
use crate::models::types::manifest::RegistryModelMetadata;
use crate::models::taxonomy::rules::{ModelCapabilities, UniversalTaskRules};
use crate::models::taxonomy::classifier::UniversalModelClassifier;

pub struct VotingArbitrator;

impl VotingArbitrator {
    /// Normalizes different task strings to standard Cluaiz tasks
    pub fn normalize_task(raw_task: &str) -> String {
        let t = raw_task.to_lowercase().replace('-', "_");
        match t.as_str() {
            "automatic_speech_recognition" | "asr" | "speech_recognition" | "speech_to_text" => {
                "automatic-speech-recognition".to_string()
            }
            "tts" | "text_to_speech" | "text_to_audio" => "text-to-speech".to_string(),
            "image_to_text" | "image_text_to_text" | "vqa" | "visual_question_answering" => {
                "image-text-to-text".to_string()
            }
            "text_generation" | "chat" | "conversational" => "chat-completion".to_string(),
            "feature_extraction" | "sentence_similarity" => "embedding".to_string(),
            "image_feature_extraction" | "zero_shot_image_classification" | "visual_document_retrieval" => {
                "image-feature-extraction".to_string()
            }
            _ => raw_task.to_string(),
        }
    }

    /// Performs 3-Way Vote to determine the actual tasks for the model
    pub fn resolve_tasks(
        model_dir: &Path,
        header_tasks: &[String],
        arch_lower: &str,
    ) -> Vec<String> {
        let mut scores: HashMap<String, u32> = HashMap::new();

        // SOURCE 1: Model Binary Header (Weight: 30)
        for t in header_tasks {
            let norm = Self::normalize_task(t);
            *scores.entry(norm).or_insert(0) += 30;
        }

        // SOURCE 2: HF API Metadata (Weight: 45)
        let hf_meta_path = model_dir.join("hf_metadata.json");
        if hf_meta_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&hf_meta_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                        let norm = Self::normalize_task(pipeline_tag);
                        *scores.entry(norm).or_insert(0) += 45;
                    }
                    if let Some(tags) = json.get("tags").and_then(|v| v.as_array()) {
                        for tag in tags {
                            if let Some(tag_str) = tag.as_str() {
                                let norm = Self::normalize_task(tag_str);
                                *scores.entry(norm).or_insert(0) += 10;
                            }
                        }
                    }
                }
            }
        }

        // SOURCE 3: Filesystem & Name Heuristics (Weight: 25)
        let dir_name = model_dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        let combined_ident = format!("{} {}", arch_lower, dir_name);

        if model_dir.join("tts.json").exists()
            || combined_ident.contains("tts")
            || combined_ident.contains("kokoro")
            || combined_ident.contains("vits")
            || combined_ident.contains("supertonic")
            || combined_ident.contains("piper")
            || combined_ident.contains("matcha")
            || combined_ident.contains("cosyvoice")
        {
            *scores.entry("text-to-speech".to_string()).or_insert(0) += 25;
        }

        if combined_ident.contains("whisper")
            || combined_ident.contains("sensevoice")
            || combined_ident.contains("moonshine")
            || combined_ident.contains("paraformer")
        {
            *scores.entry("automatic-speech-recognition".to_string()).or_insert(0) += 25;
        }

        let is_embed = combined_ident.contains("bert")
            || combined_ident.contains("embed")
            || combined_ident.contains("embedding")
            || combined_ident.contains("bge")
            || combined_ident.contains("nomic")
            || combined_ident.contains("gte")
            || combined_ident.contains("minilm");

        if is_embed {
            *scores.entry("embedding".to_string()).or_insert(0) += 40;
        }

        let is_vision_embed = combined_ident.contains("clip")
            || combined_ident.contains("siglip")
            || combined_ident.contains("colpali")
            || combined_ident.contains("colqwen");

        if is_vision_embed {
            *scores.entry("image-feature-extraction".to_string()).or_insert(0) += 40;
        }

        let mut final_tasks = Vec::new();
        let mut max_score = 0;
        for &score in scores.values() {
            if score > max_score {
                max_score = score;
            }
        }

        for (task, &score) in &scores {
            if score >= 40 || (score == max_score && max_score > 0) {
                final_tasks.push(task.clone());
            }
        }

        final_tasks.sort();
        final_tasks.dedup();
        final_tasks
    }
}

pub struct ModelProber;

impl ModelProber {
    /// Deep autonomous capability and metadata discovery on any model file and its directory
    pub fn discover(
        weight_path: &Path,
        model_dir: &Path,
        category_hint: &str,
    ) -> (SlotType, ModelCapabilities, RegistryModelMetadata, bool) {
        let is_gguf = weight_path.extension().and_then(|s| s.to_str()) == Some("gguf");
        let format_type = if is_gguf { "gguf" } else { "onnx" };

        let mut architecture = "Unknown".to_string();
        let mut context_window = "Unknown".to_string();
        let mut parameters = "Unknown".to_string();
        let mut quantization = None;
        let mut bit_depth = None;
        let mut chat_template = None;
        let mut think_start_tag = None;
        let mut think_end_tag = None;
        let mut requires_gpu = false;
        let mut header_tasks = Vec::new();

        if is_gguf {
            if let Ok(probe) = GgufProber::probe_file(weight_path) {
                if let Some(arch) = probe.architecture {
                    architecture = arch;
                }
                if let Some(ctx) = probe.context_window {
                    context_window = ctx;
                }
                if let Some(params) = probe.parameter_count {
                    parameters = params;
                }
                quantization = probe.quantization;
                chat_template = probe.chat_template;
                think_start_tag = probe.think_start_tag;
                think_end_tag = probe.think_end_tag;
            }
        } else {
            let probe = OnnxProber::probe_file(weight_path);
            let filename = weight_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            architecture = if filename.contains("kokoro") {
                "kokoro".to_string()
            } else if filename.contains("piper") {
                "piper".to_string()
            } else if filename.contains("whisper") {
                "whisper".to_string()
            } else {
                "onnx-generic".to_string()
            };
        }

        // Secondary Fallback JSON Ingestion
        let fallback = FallbackProber::probe_directory(model_dir);
        if context_window == "Unknown" || context_window.is_empty() {
            if let Some(ctx) = fallback.context_window {
                context_window = ctx;
            }
        }
        if architecture == "Unknown" || architecture.is_empty() {
            if let Some(arch) = fallback.architecture {
                architecture = arch;
            }
        }
        if chat_template.is_none() {
            chat_template = fallback.chat_template;
        }
        if think_start_tag.is_none() {
            think_start_tag = fallback.think_start_tag;
        }
        if think_end_tag.is_none() {
            think_end_tag = fallback.think_end_tag;
        }

        // 3-Way Task Voting
        let arch_lower = architecture.to_lowercase();
        let voted_tasks = VotingArbitrator::resolve_tasks(model_dir, &header_tasks, &arch_lower);

        // Classify into Sovereign Category
        let model_dir_name = model_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let filenames: Vec<String> = std::fs::read_dir(model_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();

        let classification = UniversalModelClassifier::classify(
            model_dir_name,
            voted_tasks.first().map(|s| s.as_str()),
            &voted_tasks,
            &filenames,
            Some(&architecture),
        );

        let mut caps = classification.capabilities;
        caps.explicit_tasks = voted_tasks;

        let slot = if !category_hint.is_empty() {
            SlotType::from_category(category_hint)
        } else {
            classification.slot
        };

        if context_window == "Unknown" {
            match slot {
                SlotType::Tts | SlotType::Stt => context_window = "30s (3000 frames)".to_string(),
                SlotType::Ingest | SlotType::Embedding => context_window = "224x224 (Images)".to_string(),
                _ => context_window = "8k".to_string(),
            }
        }

        let metadata = RegistryModelMetadata {
            architecture,
            parameters,
            context_window,
            quantization,
            bit_depth,
            tts_family: caps.tts_family.clone(),
            stt_family: caps.stt_family.clone(),
            backend_type: Some(format_type.to_string()),
            think_start_tag,
            think_end_tag,
            chat_template,
        };

        (slot, caps, metadata, requires_gpu)
    }
}
