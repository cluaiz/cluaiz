pub mod fallback;
pub mod prober;
pub mod rules;
pub mod arbitrator;

use crate::utils::model_registry::SlotType;
use crate::utils::{ModelCapabilities, RegistryModelMetadata};
use fallback::enrich_from_fallback_jsons;
use prober::probe_weight_binary;
use rules::{audio, chat, embedding, vision};
use arbitrator::VotingArbitrator;
use std::path::Path;

pub struct CapabilityResolver;

impl CapabilityResolver {
    pub fn discover(
        weight_path: &Path,
        model_dir: &Path,
        category_folder: &str,
    ) -> (SlotType, ModelCapabilities, RegistryModelMetadata, bool) {
        let format_type = if weight_path.extension().and_then(|s| s.to_str()) == Some("gguf") {
            "gguf"
        } else {
            "onnx"
        };

        // Level 1: Primary Weight Binary Probe
        let probe = probe_weight_binary(weight_path, format_type);
        let arch_lower = probe.architecture.to_lowercase();
        let requires_gpu = probe.requires_gpu;

        let mut caps = ModelCapabilities::default();
        
        // 🗳️ 3-WAY VOTING ARBITRATOR
        let final_tasks = VotingArbitrator::resolve_tasks(model_dir, &probe.explicit_tasks, &arch_lower);
        caps.explicit_tasks.extend(final_tasks);

        // Evaluate Rules
        let model_dir_name = model_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let combined_ident = format!("{} {}", arch_lower, model_dir_name);
        chat::evaluate_chat_rules(&combined_ident, probe.chat_template.is_some(), &mut caps);
        embedding::evaluate_embedding_rules(&combined_ident, probe.has_pooling, &mut caps);
        vision::evaluate_vision_rules(
            &combined_ident,
            probe.has_vision_keys,
            probe.has_vision_tensors,
            &mut caps,
        );
        audio::evaluate_audio_rules(
            &combined_ident,
            probe.has_audio_keys,
            probe.has_audio_tensors,
            &mut caps,
        );

        // Level 2: Secondary Fallback JSON Ingestion
        let extra_fallback = enrich_from_fallback_jsons(model_dir, &mut caps);

        let slot = if !category_folder.is_empty() {
            SlotType::from_category(category_folder)
        } else if caps.is_tts {
            SlotType::Tts
        } else if caps.is_asr {
            SlotType::Stt
        } else if caps.is_embedding || caps.is_feature_extraction {
            SlotType::Embedding
        } else if caps.has_vision && !caps.is_instruct && !caps.chat_completion {
            SlotType::Ingest
        } else {
            SlotType::Chat
        };

        let mut chat_template = probe.chat_template;
        if chat_template.is_none() {
            chat_template = extra_fallback.chat_template;
        }

        let mut think_start_tag = probe.think_start_tag;
        let mut think_end_tag = probe.think_end_tag;

        if think_start_tag.is_none() {
            think_start_tag = extra_fallback.think_start_tag;
        }
        if think_end_tag.is_none() {
            think_end_tag = extra_fallback.think_end_tag;
        }

        if slot != SlotType::Chat {
            think_start_tag = None;
            think_end_tag = None;
        }

        let mut context_window = probe.context_window;
        if (context_window == "Unknown" || context_window.is_empty()) {
            if let Some(ctx) = extra_fallback.context_window {
                context_window = ctx;
            } else if slot == SlotType::Tts || slot == SlotType::Stt {
                context_window = "30s (3000 frames)".to_string();
            } else if slot == SlotType::Ingest || slot == SlotType::Embedding {
                context_window = "224x224 (Images)".to_string();
            }
        }

        let mut parameters = probe.parameters_str;
        if (parameters == "Unknown" || parameters.is_empty()) {
            if let Some(p) = extra_fallback.parameters {
                parameters = p;
            }
        }

        let metadata = RegistryModelMetadata {
            architecture: probe.architecture,
            parameters,
            context_window,
            quantization: probe.quantization,
            bit_depth: probe.bit_depth,
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
