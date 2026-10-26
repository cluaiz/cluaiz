//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: Deep Binary Inspection & Autonomous Capability Discovery (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

pub mod fallback;
pub mod gguf;
pub mod onnx;

pub use fallback::{FallbackJsonMetadata, FallbackProber};
pub use gguf::{GgufProbeResult, GgufProber};
pub use onnx::{OnnxProbeResult, OnnxProber};

use crate::models::taxonomy::classifier::UniversalModelClassifier;
use crate::models::taxonomy::rules::ModelCapabilities;
use crate::models::types::entities::SlotType;
use crate::models::types::manifest::RegistryModelMetadata;
use std::collections::HashMap;
use std::path::Path;

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
            "image_feature_extraction"
            | "zero_shot_image_classification"
            | "visual_document_retrieval" => "image-feature-extraction".to_string(),
            _ => raw_task.to_string(),
        }
    }

    /// Tasks a model declares: its weights' header (GGUF / ONNX), the Hugging Face pipeline tag
    /// and tags saved with it (`hf_metadata.json`), and a `tts.json` beside it. Names of models
    /// and files are never read.
    pub fn resolve_tasks(model_dir: &Path, header_tasks: &[String]) -> Vec<String> {
        let mut scores: HashMap<String, u32> = HashMap::new();

        // SOURCE 1: Model Binary Header (Weight: 30)
        for t in header_tasks {
            let norm = Self::normalize_task(t);
            *scores.entry(norm).or_insert(0) += 30;
        }

        // SOURCE 2: HF API Metadata (Weight: 45)
        let hf_meta_path = model_dir.join("hf_metadata.json");
        if let Ok(content) = std::fs::read_to_string(&hf_meta_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                    *scores.entry(Self::normalize_task(pipeline_tag)).or_insert(0) += 45;
                }
                for tag in json.get("tags").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|t| t.as_str()) {
                    *scores.entry(Self::normalize_task(tag)).or_insert(0) += 10;
                }
            }
        }

        // SOURCE 3: A speech configuration file beside the weights (Weight: 25)
        if model_dir.join("tts.json").exists() {
            *scores.entry("text-to-speech".to_string()).or_insert(0) += 25;
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
        let bit_depth = None;
        let mut chat_template = None;
        let mut think_start_tag = None;
        let mut think_end_tag = None;
        let requires_gpu = false;
        let mut header_tasks = Vec::new();
        let mut primary_header = None;
        let mut onnx_package = None;

        if is_gguf {
            if let Ok(header) = GgufProber::read_header(weight_path) {
                let probe = GgufProber::probe_header(&header);
                if probe.is_embedding {
                    header_tasks.push("embedding".to_string());
                }
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
                primary_header = Some(header);
            }
        } else {
            architecture = "onnx".to_owned();
            onnx_package =
                crate::capabilities::PackageCapabilities::from_onnx(weight_path, model_dir).ok();
            if let Some(package) = &onnx_package {
                header_tasks = package.onnx_tasks();
            }
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
        let voted_tasks = VotingArbitrator::resolve_tasks(model_dir, &header_tasks);

        // Classify from the declared tasks (never from names)
        let classification = UniversalModelClassifier::classify(voted_tasks.first().map(|s| s.as_str()), &voted_tasks);

        let mut caps = classification.capabilities;
        caps.explicit_tasks = voted_tasks;

        let slot = if !category_hint.is_empty() {
            SlotType::from_category(category_hint)
        } else {
            classification.slot
        };

        let package = onnx_package.or_else(|| {
            primary_header.as_ref().map(|header| {
                crate::capabilities::PackageCapabilities::from_header(
                    weight_path,
                    model_dir,
                    header,
                )
            })
        });
        if !is_gguf {
            caps.explicit_tasks = package.as_ref().map(|p| p.onnx_tasks()).unwrap_or_default();
            context_window = "Unknown".into();
        }
        if let Some(package) = package.as_ref() {
            let inputs = package.supported_inputs();
            caps.has_vision = inputs.contains(&cluaiz_core::Modality::Image);
            caps.has_audio = inputs.contains(&cluaiz_core::Modality::Audio);
            caps.is_vision_chat = caps.has_vision && slot == SlotType::Chat;
        } else if is_gguf {
            caps.has_vision = false;
            caps.has_audio = false;
            caps.is_vision_chat = false;
        }

        if context_window == "Unknown" && is_gguf {
            match slot {
                SlotType::Tts | SlotType::Stt => context_window = "30s (3000 frames)".to_string(),
                SlotType::Ingest | SlotType::Embedding => {
                    context_window = "224x224 (Images)".to_string()
                }
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
            capabilities: package,
        };

        (slot, caps, metadata, requires_gpu)
    }
}
