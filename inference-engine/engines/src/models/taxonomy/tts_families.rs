//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal TTS Families, Scoring & Packaging Contracts (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;
use crate::models::taxonomy::quantization::UniversalQuantization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsFamily {
    Piper,
    Kokoro,
    Supertonic,
    Audio8,
    Matcha,
    CosyVoice,
    Chatterbox,
    OmniVoice,
    ParlerTts,
    ChatTts,
    Vits,
    Bark,
    F5Tts,
    Whisper,
    GenericOnnxTts,
    Unknown,
}

impl TtsFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            TtsFamily::Piper => "piper-vits",
            TtsFamily::Kokoro => "kokoro-v1",
            TtsFamily::Supertonic => "supertonic-v3",
            TtsFamily::Audio8 => "audio8-codec",
            TtsFamily::Matcha => "matcha-v1",
            TtsFamily::CosyVoice => "cosyvoice",
            TtsFamily::Chatterbox => "chatterbox",
            TtsFamily::OmniVoice => "omnivoice",
            TtsFamily::ParlerTts => "parler_tts",
            TtsFamily::ChatTts => "chattts",
            TtsFamily::Vits => "vits",
            TtsFamily::Bark => "bark",
            TtsFamily::F5Tts => "f5_tts",
            TtsFamily::Whisper => "whisper-gguf",
            TtsFamily::GenericOnnxTts => "generic-onnx-tts",
            TtsFamily::Unknown => "unknown",
        }
    }
}

pub struct TtsTaxonomy;

impl TtsTaxonomy {
    /// Evaluates whether a filename or path is a TTS-specific asset, config, or directory payload.
    pub fn is_tts_asset(filename: &str) -> bool {
        let name = filename.to_lowercase();
        name.contains("voices/")
            || name.ends_with(".bin")
            || name.ends_with(".onnx.json")
            || name.ends_with(".onnx.yaml")
            || name.ends_with("tokens.txt")
            || name.ends_with("lexicon.txt")
            || name.ends_with("voices.json")
            || name.ends_with("tts.json")
            || name.ends_with("unicode_indexer.json")
            || name.contains("voice_styles/")
            || name.contains("espeak-ng-data")
    }

    /// Strips quantization suffix and extension to return the normalized model base stem using Universal SSOT.
    pub fn strip_quant(p: &str) -> String {
        UniversalQuantization::strip_quant(p)
    }

    /// Evaluates whether two ONNX paths represent the same model graph under different quantizations.
    pub fn is_same_model_different_quant(path_a: &str, path_b: &str) -> bool {
        let dir_a = Path::new(path_a).parent().and_then(|p| p.to_str()).unwrap_or("");
        let dir_b = Path::new(path_b).parent().and_then(|p| p.to_str()).unwrap_or("");

        if dir_a != dir_b {
            return false;
        }

        let base_a = UniversalQuantization::strip_quant(path_a);
        let base_b = UniversalQuantization::strip_quant(path_b);

        base_a == base_b && path_a != path_b
    }

    /// Evaluates whether two candidate model paths belong to different architectural scales (e.g. base/ vs base_small/).
    pub fn is_scale_mismatch(path_a: &str, path_b: &str) -> bool {
        let scale_dirs = ["base", "base_small", "base_large", "small", "medium", "large", "custom", "custom_small"];
        let get_scale = |p: &str| -> Option<String> {
            p.split('/')
                .find(|part| scale_dirs.contains(&part.to_lowercase().as_str()))
                .map(|s| s.to_lowercase())
        };
        let primary_scale = get_scale(path_a);
        let candidate_scale = get_scale(path_b);
        primary_scale.is_some() && candidate_scale.is_some() && primary_scale != candidate_scale
    }

    /// Evaluates whether a model file is a sub-component stage (vocoder, denoiser, duration predictor, speech tokenizer)
    /// that cannot function as an independent standalone model variant.
    pub fn is_subcomponent_file(filename: &str) -> bool {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        basename.contains("denoiser")
            || basename.contains("duration_predictor")
            || basename.contains("vector_estimator")
            || basename.contains("vector_context_encoder")
            || basename.contains("vocoder_adapter")
            || basename.contains("vocoder_b")
            || basename.contains("speech_tokenizer")
            || basename.contains("speaker_encoder")
            || basename.contains("campplus")
            || basename.contains("hifigan")
            || basename.contains("hift")
            || basename.contains("istftnet2")
            || basename.contains("codec_decoder")
    }

    /// Evaluates whether an ONNX file is a valid primary entrypoint that can anchor a variant bundle.
    pub fn is_primary_entrypoint(filename: &str) -> bool {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        if !basename.ends_with(".onnx") {
            return false;
        }

        if basename.contains(".onnx.data") || basename.contains("vocab") {
            return false;
        }

        !Self::is_subcomponent_file(filename)
            || basename.contains("acoustic")
            || basename.contains("backbone")
            || basename.contains("model.onnx")
            || basename.contains("model_")
            || basename.contains("kokoro")
            || basename.contains("synth")
    }

    /// Determines the exact TTS Family tag from repo/model identifiers and local files (Universal Taxonomy Engine).
    pub fn detect_family(repo_or_dir: &str, file_inventory: &str) -> TtsFamily {
        let lower = repo_or_dir.to_lowercase();
        let files_joined = file_inventory.to_lowercase();
        let combined = format!("{} {}", lower, files_joined);

        if combined.contains("kokoro") || files_joined.contains("voices/") {
            TtsFamily::Kokoro
        } else if combined.contains("supertonic") || files_joined.contains("vector_estimator") || files_joined.contains("duration_predictor") {
            TtsFamily::Supertonic
        } else if combined.contains("matcha") || files_joined.contains("matcha_acoustic") || files_joined.contains("matxa") {
            TtsFamily::Matcha
        } else if combined.contains("cosyvoice") || combined.contains("cosy_voice") || files_joined.contains("campplus") || files_joined.contains("speech_llm") {
            TtsFamily::CosyVoice
        } else if combined.contains("audio8") || files_joined.contains("slow_ar") || files_joined.contains("fast_ar") {
            TtsFamily::Audio8
        } else if combined.contains("chatterbox") || files_joined.contains("chatterbox_generator") {
            TtsFamily::Chatterbox
        } else if combined.contains("omnivoice") || files_joined.contains("genai_config") {
            TtsFamily::OmniVoice
        } else if combined.contains("vits") || combined.contains("piper") || combined.contains("mms-tts") || files_joined.contains("phonemes.json") {
            TtsFamily::Piper
        } else if combined.contains("chattts") || combined.contains("chat_tts") {
            TtsFamily::ChatTts
        } else if combined.contains("parler") {
            TtsFamily::ParlerTts
        } else if combined.contains("bark") {
            TtsFamily::Bark
        } else if combined.contains("f5") {
            TtsFamily::F5Tts
        } else if combined.contains("whisper") {
            TtsFamily::Whisper
        } else if files_joined.ends_with(".onnx") || files_joined.contains(".onnx") {
            TtsFamily::GenericOnnxTts
        } else {
            TtsFamily::Unknown
        }
    }

    /// Evaluates candidate TTS model files for priority scoring (0 = Primary Graph, 1 = Sub-Graph/Encoder, 2 = Vocoder, 3 = Helper/Speaker).
    pub fn score_tts_file(filename: &str) -> usize {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        // Priority 0: Primary Generators / Encoders / Flow Estimators / Acoustic Graphs / AR Generators
        if basename.contains("slow_ar")
            || basename.contains("llm_decoder")
            || basename.contains("speech_llm")
            || basename.contains("flow")
            || basename.contains("estimator")
            || basename.contains("generator")
            || basename.contains("acoustic")
            || basename.contains("synth")
            || basename.contains("matcha")
            || basename == "model.onnx"
            || basename == "model_q4.onnx"
            || basename == "kokoro-82m.onnx"
            || basename == "kokoro-v0_19.onnx"
            || basename == "kokoro.onnx"
            || basename == "text_encoder.onnx"
        {
            0
        }
        // Priority 1: Secondary Sub-Graphs / Duration Predictor / Fast AR / Decoders / Encoders
        else if basename.contains("duration_predictor")
            || basename.contains("vector_estimator")
            || basename.contains("fast_ar")
            || basename.contains("decoder")
            || basename.contains("encoder")
        {
            1
        }
        // Priority 2: Neural Vocoders & Audio Codec Decoders (hifigan, hift, codec_decoder, audio_codec)
        else if basename.contains("vocoder")
            || basename.contains("hifigan")
            || basename.contains("hift")
            || basename.contains("codec_decoder")
            || basename.contains("audio_codec")
            || basename.contains("wavenext")
        {
            2
        }
        // Priority 3: Speaker Embeddings / CampPlus
        else if basename.contains("campplus")
            || basename.contains("speaker")
            || basename.contains("embed")
        {
            3
        } else {
            1
        }
    }

    /// Pre-boot / Post-download Dynamic Package Contract Validation Gate (CERD Law Compliance).
    /// Uses Wildcard Directory Pattern Matching & .onnx.data checks instead of hardcoded strings to ensure bulletproof validation
    /// across 800+ HuggingFace repos, variable file names, and hybrid quantizations.
    pub fn validate_family_contract(family: TtsFamily, local_dir: &Path) -> Result<(), String> {
        let mut onnx_files = Vec::new();
        let mut json_files = Vec::new();
        let mut data_files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(local_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if name.ends_with(".onnx") {
                    onnx_files.push(name);
                } else if name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".yml") {
                    json_files.push(name);
                } else if name.ends_with(".onnx.data") || name.ends_with(".data") {
                    data_files.push(name);
                }
            }
        }

        // Helper to verify if large ONNX models (>1.8GB Protobuf limit) have matching .onnx.data companion
        let check_data_companion = |primary_onnx: &str| -> Result<(), String> {
            let data_name = format!("{}.data", primary_onnx);
            let alt_data_name = format!("{}_data", primary_onnx);
            let has_companion = data_files.iter().any(|d| d == &data_name || d == &alt_data_name || d == "model.onnx.data");

            let primary_path = local_dir.join(primary_onnx);
            if primary_path.exists() {
                let size_bytes = primary_path.metadata().map(|m| m.len()).unwrap_or(0);
                if size_bytes > 1_800_000_000 && !has_companion && data_files.is_empty() {
                    return Err(format!(
                        "ONNX Weight Pointer Exception: '{}' is >1.8GB but companion '.onnx.data' weight file is missing!",
                        primary_onnx
                    ));
                }
            }
            Ok(())
        };

        let config_or_tokens_present = |json_list: &[String], dir: &Path| -> bool {
            if json_list.iter().any(|f| f.contains("config") || f.contains("tokens") || f.contains("phonemes") || f.contains("vocab") || f.contains("meta")) {
                return true;
            }
            dir.join("tokens.txt").exists() || dir.join("lexicon.txt").exists()
        };

        match family {
            TtsFamily::Kokoro => {
                let primary_model = onnx_files.iter().find(|f| {
                    f.contains("kokoro") || f.contains("model") || f.as_str() == "model.onnx" || f.as_str() == "model_q4.onnx"
                });
                if primary_model.is_none() {
                    return Err("Kokoro Contract Violation: Missing primary ONNX model graph (*kokoro*.onnx or model*.onnx).".to_string());
                }
                check_data_companion(primary_model.unwrap())?;

                let has_config = json_files.iter().any(|f| f.contains("config"));
                let has_tokenizer = json_files.iter().any(|f| f.contains("tokenizer") || f.contains("vocab") || f.contains("tokens"));
                if !has_config || !has_tokenizer {
                    return Err("Kokoro Contract Violation: Missing config.json or tokenizer.json / vocab.json.".to_string());
                }

                // Bulletproof check against empty dir buzz noise
                let voices_dir = local_dir.join("voices");
                let has_non_empty_voices = voices_dir.exists()
                    && voices_dir.is_dir()
                    && std::fs::read_dir(&voices_dir)
                        .map(|mut i| {
                            i.any(|e| {
                                e.ok().map_or(false, |entry| {
                                    let n = entry.file_name().to_string_lossy().to_lowercase();
                                    n.ends_with(".bin") || n.ends_with(".json") || n.ends_with(".pt")
                                })
                            })
                        })
                        .unwrap_or(false);

                if !has_non_empty_voices {
                    return Err("Kokoro Contract Violation: 'voices/' directory is missing or empty! Style vectors (.bin / .json) are required.".to_string());
                }
            }

            TtsFamily::Supertonic => {
                let text_enc = onnx_files.iter().find(|f| f.contains("text_encoder") || f.contains("encoder"));
                let dur_pred = onnx_files.iter().find(|f| f.contains("duration_predictor") || f.contains("duration"));
                let vec_est = onnx_files.iter().find(|f| f.contains("vector_estimator") || f.contains("estimator") || f.contains("denoiser"));
                let vocoder = onnx_files.iter().find(|f| f.contains("vocoder") || f.contains("hift") || f.contains("decoder"));

                if text_enc.is_none() || dur_pred.is_none() || vec_est.is_none() || vocoder.is_none() {
                    return Err("Supertonic Contract Violation: Missing 1 or more of the 4 mandatory sub-graphs (text_encoder, duration_predictor, vector_estimator, vocoder).".to_string());
                }
                check_data_companion(vec_est.unwrap())?;
            }

            TtsFamily::Matcha => {
                let acoustic = onnx_files.iter().find(|f| f.contains("matcha") || f.contains("acoustic") || f.contains("estimator") || f.contains("flow") || f.contains("matxa"));
                let vocoder = onnx_files.iter().find(|f| f.contains("hifigan") || f.contains("vocoder") || f.contains("wavenext"));

                if acoustic.is_none() || vocoder.is_none() {
                    return Err("Matcha-TTS Contract Violation: Missing acoustic flow estimator or HiFi-GAN neural vocoder sub-graph.".to_string());
                }
                check_data_companion(acoustic.unwrap())?;
            }

            TtsFamily::Piper | TtsFamily::Vits => {
                if onnx_files.is_empty() || !config_or_tokens_present(&json_files, local_dir) {
                    return Err("Piper/VITS Contract Violation: Missing primary model.onnx or config/tokens metadata.".to_string());
                }
                check_data_companion(&onnx_files[0])?;
            }

            TtsFamily::Audio8 => {
                let generator = onnx_files.iter().find(|f| f.contains("slow_ar") || f.contains("generator"));
                let codec_decoder = onnx_files.iter().find(|f| f.contains("codec_decoder") || f.contains("decoder") || f.contains("vocoder"));

                if generator.is_none() || codec_decoder.is_none() {
                    return Err("Audio8 Contract Violation: Missing slow_ar generator or codec_decoder FP16 neural vocoder.".to_string());
                }
                check_data_companion(generator.unwrap())?;
            }

            TtsFamily::CosyVoice => {
                let speech_llm = onnx_files.iter().find(|f| f.contains("speech_llm") || f.contains("llm"));
                let flow = onnx_files.iter().find(|f| f.contains("flow"));
                let _hift = onnx_files.iter().find(|f| f.contains("hift") || f.contains("vocoder"));
                let _campplus = onnx_files.iter().find(|f| f.contains("campplus") || f.contains("speaker"));
                let has_combined = onnx_files.iter().any(|f| f.contains("combined"));

                if speech_llm.is_none() && !has_combined && flow.is_none() {
                    return Err("CosyVoice Contract Violation: Missing speech_llm, flow_model/hift_vocoder, or campplus speaker extractor.".to_string());
                }
                if let Some(llm) = speech_llm {
                    check_data_companion(llm)?;
                }
            }

            TtsFamily::Chatterbox => {
                let gen = onnx_files.iter().find(|f| f.contains("chatterbox") || f.contains("generator") || f.contains("decoder") || f.contains("language_model"));
                let codec = onnx_files.iter().find(|f| f.contains("codec") || f.contains("speech_encoder") || f.contains("embed"));

                if gen.is_none() || codec.is_none() {
                    return Err("Chatterbox Contract Violation: Missing semantic generator or audio codec sub-graph.".to_string());
                }
            }

            TtsFamily::OmniVoice => {
                let has_genai_data = local_dir.join("model.onnx.data").exists() || !data_files.is_empty();
                let has_genai_config = json_files.iter().any(|f| f.contains("genai_config") || f.contains("config"));

                if !has_genai_data || !has_genai_config {
                    return Err("OmniVoice Contract Violation: Missing mandatory model.onnx.data external weights file or genai_config.json.".to_string());
                }
            }

            _ => {
                if onnx_files.is_empty() {
                    return Err("Generic ONNX TTS Contract Violation: Missing primary .onnx graph file.".to_string());
                }
                if !config_or_tokens_present(&json_files, local_dir) {
                    return Err("Generic ONNX TTS Contract Violation: Missing configuration JSON or tokens metadata file.".to_string());
                }
            }
        }
        Ok(())
    }
}
