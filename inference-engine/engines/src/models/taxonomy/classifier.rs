//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Model Classifier (Single Source of Truth)
//! ═══════════════════════════════════════════════════════════════════════

use cluaiz_shared::utils::model_registry::{ModelCapabilities, SlotType};
use crate::models::taxonomy::tags::*;
use crate::models::taxonomy::heuristics::*;
use crate::models::taxonomy::tts_families::{TtsFamily, TtsTaxonomy};

#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub slot: SlotType,
    pub category: String,
    pub supported_tasks: Vec<String>,
    pub capabilities: ModelCapabilities,
    pub tts_family: Option<String>,
}

pub struct UniversalModelClassifier;

impl UniversalModelClassifier {
    /// Classifies any model deterministically into its sovereign category and task list.
    ///
    /// Priority Order:
    /// 1. Explicit Hugging Face pipeline tag (if provided)
    /// 2. Audio & Speech identification (TTS / ASR / Vocoder)
    /// 3. Text Embedding identification (sentence-similarity / feature-extraction / bert / embedding keywords)
    /// 4. Vision & Multimodal identification (clip / siglip / vision-chat)
    /// 5. Chat / Causal LM (default conversational fallback)
    pub fn classify(
        identifier: &str,
        pipeline_tag: Option<&str>,
        additional_tags: &[String],
        filenames: &[String],
        arch_hint: Option<&str>,
    ) -> ClassificationResult {
        let tag_clean = pipeline_tag.unwrap_or("").trim().to_lowercase();
        let ident_lower = identifier.to_lowercase();
        let arch_lower = arch_hint.unwrap_or("").to_lowercase();
        let files_joined = filenames.join(" ").to_lowercase();

        let combined_text = format!("{} {} {} {}", ident_lower, arch_lower, tag_clean, files_joined);

        // ── 1. Check Audio (TTS vs STT) ──
        let is_hf_tts = HF_TAGS_TTS.iter().any(|&t| tag_clean == t);
        let is_hf_asr = HF_TAGS_ASR.iter().any(|&t| tag_clean == t);
        let is_hf_audio_gen = HF_TAGS_AUDIO_GENERAL.iter().any(|&t| tag_clean == t);
        let is_tts_kw = is_tts_ident(&combined_text);
        let is_asr_kw = is_asr_ident(&combined_text);

        if is_hf_tts || is_tts_kw {
            let mut caps = ModelCapabilities::default();
            caps.is_tts = true;
            let tasks = vec!["text-to-speech".to_string()];
            let mut tts_fam = None;
            let fam = TtsTaxonomy::detect_family(&ident_lower, &files_joined);
            if fam != TtsFamily::Unknown {
                tts_fam = Some(fam.as_str().to_string());
            }
            caps.tts_family = tts_fam.clone();

            return ClassificationResult {
                slot: SlotType::Tts,
                category: "tts".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: tts_fam,
            };
        } else if is_hf_asr || is_asr_kw || is_hf_audio_gen || matches_any(&combined_text, AUDIO_CONVERSION_KEYWORDS) {
            let mut caps = ModelCapabilities::default();
            caps.is_asr = true;
            let tasks = vec!["automatic-speech-recognition".to_string()];

            return ClassificationResult {
                slot: SlotType::Stt,
                category: "stt".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: None,
            };
        }

        // ── 2. Check Text Embedding ──
        let is_hf_embedding = HF_TAGS_EMBEDDING.iter().any(|&t| tag_clean == t)
            || additional_tags.iter().any(|t| {
                let tl = t.to_lowercase();
                tl == "sentence-similarity" || tl == "feature-extraction" || tl == "embeddings"
            });
        let is_embed_kw = is_embedding_ident(&ident_lower) || is_embedding_ident(&arch_lower);

        if is_hf_embedding || is_embed_kw {
            let mut caps = ModelCapabilities::default();
            caps.is_embedding = true;
            caps.is_feature_extraction = true;
            let tasks = vec!["embedding".to_string(), "feature-extraction".to_string()];

            return ClassificationResult {
                slot: SlotType::TextEmbedding,
                category: "text-embedding".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: None,
            };
        }

        // ── 3. Check Vision: Vision-Embedding (CLIP/SigLIP) vs Vision-Ingest (OCR/VLM) ──
        let is_hf_vision = HF_TAGS_VISION.iter().any(|&t| tag_clean == t);
        let is_vision_kw = is_vision_ident(&combined_text);

        if is_hf_vision || is_vision_kw {
            let is_vision_embed = combined_text.contains("clip") 
                || combined_text.contains("siglip") 
                || combined_text.contains("colpali") 
                || combined_text.contains("image-text-similarity");

            if is_vision_embed {
                let mut caps = ModelCapabilities::default();
                caps.has_vision = true;
                caps.is_embedding = true;
                let tasks = vec!["vision-embedding".to_string(), "feature-extraction".to_string()];

                return ClassificationResult {
                    slot: SlotType::VisionEmbedding,
                    category: "vision-embedding".to_string(),
                    supported_tasks: tasks,
                    capabilities: caps,
                    tts_family: None,
                };
            } else {
                let mut caps = ModelCapabilities::default();
                caps.has_vision = true;
                caps.is_vision_chat = true;
                caps.is_image_to_text = true;
                let tasks = vec!["image-to-text".to_string(), "document-ingestion".to_string(), "visual-qa".to_string()];

                return ClassificationResult {
                    slot: SlotType::VisionIngest,
                    category: "vision-ingest".to_string(),
                    supported_tasks: tasks,
                    capabilities: caps,
                    tts_family: None,
                };
            }
        }

        // ── 4. Default: Chat / Conversational LLM ──
        let mut caps = ModelCapabilities::default();
        caps.chat_completion = true;
        let tasks = vec!["chat-completion".to_string()];

        ClassificationResult {
            slot: SlotType::Chat,
            category: "chat".to_string(),
            supported_tasks: tasks,
            capabilities: caps,
            tts_family: None,
        }
    }
}
