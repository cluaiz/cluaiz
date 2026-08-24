//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Model Classifier (Single Source of Truth)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::types::entities::SlotType;
use crate::models::taxonomy::rules::ModelCapabilities;
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
        let is_hf_stt = HF_TAGS_STT.iter().any(|&t| tag_clean == t);
        let is_tts_kw = is_tts_ident(&combined_text);
        let is_asr_kw = is_asr_ident(&combined_text);

        if is_hf_tts || is_tts_kw {
            let mut caps = ModelCapabilities::default();
            caps.is_tts = true;
            let tasks = vec!["text-to-speech".to_string(), "voice-synthesis".to_string()];
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
        } else if is_hf_stt || is_asr_kw || matches_any(&combined_text, AUDIO_CONVERSION_KEYWORDS) {
            let mut caps = ModelCapabilities::default();
            caps.is_asr = true;
            let tasks = vec!["automatic-speech-recognition".to_string(), "audio-classification".to_string()];

            return ClassificationResult {
                slot: SlotType::Stt,
                category: "stt".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: None,
            };
        }

        // ── 2. Check Embedding (Unified Text & Vision) ──
        let is_hf_embedding = HF_TAGS_EMBEDDING.iter().any(|&t| tag_clean == t)
            || additional_tags.iter().any(|t| {
                let tl = t.to_lowercase();
                tl == "sentence-similarity" || tl == "feature-extraction" || tl == "embeddings" || tl == "embedding"
            });
        let is_embed_kw = is_embedding_ident(&combined_text);

        if is_hf_embedding || is_embed_kw {
            let mut caps = ModelCapabilities::default();
            caps.is_embedding = true;
            caps.is_feature_extraction = true;
            let tasks = vec![
                "sentence-similarity".to_string(),
                "feature-extraction".to_string(),
                "visual-document-retrieval".to_string(),
            ];

            return ClassificationResult {
                slot: SlotType::Embedding,
                category: "embedding".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: None,
            };
        }

        // ── 3. Check Ingest (Document AI / OCR / SAM) ──
        let is_hf_ingest = HF_TAGS_INGEST.iter().any(|&t| tag_clean == t);
        let is_ingest_kw = is_ingest_ident(&combined_text);

        if is_hf_ingest || is_ingest_kw {
            let mut caps = ModelCapabilities::default();
            caps.has_vision = true;
            let tasks = vec![
                "document-ocr".to_string(),
                "table-extraction".to_string(),
                "object-detection".to_string(),
            ];

            return ClassificationResult {
                slot: SlotType::Ingest,
                category: "ingest".to_string(),
                supported_tasks: tasks,
                capabilities: caps,
                tts_family: None,
            };
        }

        // ── 4. Multimodal VLM Chat vs Standard Text Chat ──
        let is_vlm = is_vlm_chat_ident(&combined_text) || tag_clean == "image-text-to-text" || tag_clean == "image-to-text";
        let mut caps = ModelCapabilities::default();
        caps.chat_completion = true;
        let mut tasks = vec!["chat-completion".to_string(), "text-generation".to_string()];
        if is_vlm {
            caps.has_vision = true;
            caps.is_vision_chat = true;
            tasks.push("image-text-to-text".to_string());
            tasks.push("image-to-text".to_string());
            tasks.push("visual-question-answering".to_string());
        }

        ClassificationResult {
            slot: SlotType::Chat,
            category: "chat".to_string(),
            supported_tasks: tasks,
            capabilities: caps,
            tts_family: None,
        }
    }
}
