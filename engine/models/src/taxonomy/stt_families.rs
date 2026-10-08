//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal STT / ASR Families, Scoring & Packaging Contracts (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;
use crate::models::taxonomy::quantization::UniversalQuantization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttFamily {
    WhisperGguf,
    WhisperOnnx,
    SenseVoice,
    Moonshine,
    Paraformer,
    Zipformer,
    Wav2Vec2,
    GenericOnnxStt,
    Unknown,
}

impl SttFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            SttFamily::WhisperGguf => "whisper-gguf",
            SttFamily::WhisperOnnx => "whisper-onnx",
            SttFamily::SenseVoice => "sensevoice",
            SttFamily::Moonshine => "moonshine",
            SttFamily::Paraformer => "paraformer",
            SttFamily::Zipformer => "zipformer",
            SttFamily::Wav2Vec2 => "wav2vec2",
            SttFamily::GenericOnnxStt => "generic-onnx-stt",
            SttFamily::Unknown => "unknown",
        }
    }
}

pub struct SttTaxonomy;

impl SttTaxonomy {
    /// Evaluates whether a filename or path is an STT/ASR-specific asset, config, or normalization artifact.
    pub fn is_stt_asset(filename: &str) -> bool {
        let name = filename.to_lowercase();
        name.ends_with(".mvn")
            || name.ends_with("am.mvn")
            || name.ends_with("feat.mvn")
            || name.ends_with("tokens.txt")
            || name.ends_with("vocab.json")
            || name.ends_with("tokenizer.json")
            || name.ends_with("preprocessor_config.json")
            || name.ends_with("generation_config.json")
            || name.contains("silero_vad")
            || name.contains("vad.")
    }

    /// Strips quantization suffix and extension to return the normalized model base stem using Universal SSOT.
    pub fn strip_quant(p: &str) -> String {
        UniversalQuantization::strip_quant(p)
    }

    /// Evaluates whether a model file is an STT sub-component stage (preprocessor, decoder cache, joiner).
    pub fn is_subcomponent_file(filename: &str) -> bool {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        basename.contains("preprocess")
            || basename.contains("cached_decode")
            || basename.contains("uncached_decode")
            || basename.contains("decoder")
            || basename.contains("joiner")
            || basename.contains("vad")
            || basename.contains("feature_extractor")
    }

    /// Evaluates whether an ONNX/GGUF file is a valid primary entrypoint for STT.
    pub fn is_primary_entrypoint(filename: &str) -> bool {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        if basename.ends_with(".gguf") || basename.ends_with(".bin") {
            return basename.contains("ggml") || basename.contains("whisper");
        }

        if !basename.ends_with(".onnx") {
            return false;
        }

        if basename.contains(".onnx.data") || basename.contains("vocab") {
            return false;
        }

        basename.contains("encoder")
            || basename.contains("model.onnx")
            || basename.contains("model_")
            || basename.contains("sensevoice")
            || basename.contains("whisper")
            || !Self::is_subcomponent_file(filename)
    }

    /// Determines the exact STT Family tag from repo/model identifiers and local files.
    pub fn detect_family(repo_or_dir: &str, file_inventory: &str) -> SttFamily {
        let lower = repo_or_dir.to_lowercase();
        let files_joined = file_inventory.to_lowercase();
        let combined = format!("{} {}", lower, files_joined);

        if combined.contains("sensevoice") || files_joined.contains("am.mvn") {
            SttFamily::SenseVoice
        } else if combined.contains("moonshine") || files_joined.contains("cached_decode") {
            SttFamily::Moonshine
        } else if combined.contains("paraformer") {
            SttFamily::Paraformer
        } else if combined.contains("zipformer") || files_joined.contains("joiner") {
            SttFamily::Zipformer
        } else if combined.contains("wav2vec2") || combined.contains("mms-1b-all") || combined.contains("mms-300m") {
            SttFamily::Wav2Vec2
        } else if combined.contains("whisper") {
            if files_joined.contains(".onnx") {
                SttFamily::WhisperOnnx
            } else {
                SttFamily::WhisperGguf
            }
        } else if files_joined.contains(".onnx") {
            SttFamily::GenericOnnxStt
        } else {
            SttFamily::Unknown
        }
    }

    /// Evaluates candidate STT model files for priority scoring (0 = Primary Acoustic / Encoder, 1 = Decoder / Joiner, 2 = Preprocessor / Feature Extractor).
    pub fn score_stt_file_priority(filename: &str) -> usize {
        let name = filename.to_lowercase();
        let basename = Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);

        // Priority 0: Primary Acoustic Encoder / Monolithic Models / Whisper Graphs
        if basename.contains("encoder")
            || basename.contains("sensevoice")
            || basename.contains("model.onnx")
            || basename.contains("model_q4.onnx")
            || basename.contains("ggml-")
            || basename.contains("whisper")
        {
            0
        }
        // Priority 1: Secondary Autoregressive Decoders / Transducer Joiners
        else if basename.contains("decoder")
            || basename.contains("joiner")
            || basename.contains("cached_decode")
            || basename.contains("uncached_decode")
        {
            1
        }
        // Priority 2: Audio Preprocessors / VAD Filter / Feature Extractors
        else if basename.contains("preprocess")
            || basename.contains("vad")
            || basename.contains("feature")
        {
            2
        } else {
            1
        }
    }

    /// Pre-boot / Post-download STT Package Contract Validation Gate (CERD Law Compliance).
    pub fn validate_family_contract(family: SttFamily, local_dir: &Path) -> Result<(), String> {
        let mut onnx_files = Vec::new();
        let mut gguf_bin_files = Vec::new();
        let mut json_yaml_files = Vec::new();
        let mut data_files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(local_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if name.ends_with(".onnx") {
                    onnx_files.push(name);
                } else if name.ends_with(".gguf") || name.ends_with(".bin") {
                    gguf_bin_files.push(name);
                } else if name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".yml") || name.ends_with(".txt") {
                    json_yaml_files.push(name);
                } else if name.ends_with(".onnx.data") || name.ends_with(".data") {
                    data_files.push(name);
                }
            }
        }

        match family {
            SttFamily::WhisperGguf => {
                let has_weights = gguf_bin_files.iter().any(|f| f.contains("ggml") || f.contains("whisper") || f.ends_with(".bin") || f.ends_with(".gguf"));
                if !has_weights {
                    return Err("Whisper GGUF Contract Violation: Missing core ggml-*.bin or *.gguf weight binary.".to_string());
                }
            }

            SttFamily::WhisperOnnx => {
                let has_encoder = onnx_files.iter().any(|f| f.contains("encoder"));
                let has_decoder = onnx_files.iter().any(|f| f.contains("decoder"));
                let has_tokenizer = json_yaml_files.iter().any(|f| f.contains("tokenizer") || f.contains("tokens") || f.contains("vocab"));

                if (!has_encoder || !has_decoder) && onnx_files.is_empty() {
                    return Err("Whisper ONNX Contract Violation: Missing encoder_model.onnx or decoder_model.onnx graphs.".to_string());
                }
                if !has_tokenizer {
                    return Err("Whisper ONNX Contract Violation: Missing tokenizer.json / vocab.json metadata.".to_string());
                }
            }

            SttFamily::SenseVoice => {
                let has_model = onnx_files.iter().any(|f| f.contains("model") || f.contains("sensevoice"));
                let has_mvn = local_dir.join("am.mvn").exists() || json_yaml_files.iter().any(|f| f.contains("mvn"));

                if !has_model {
                    return Err("SenseVoice Contract Violation: Missing core model.onnx acoustic graph.".to_string());
                }
                if !has_mvn {
                    return Err("SenseVoice Contract Violation: Missing mandatory am.mvn acoustic normalization vector.".to_string());
                }
            }

            SttFamily::Moonshine => {
                let has_encoder = onnx_files.iter().any(|f| f.contains("encoder"));
                let has_decoder = onnx_files.iter().any(|f| f.contains("decode"));

                if !has_encoder || !has_decoder {
                    return Err("Moonshine Contract Violation: Missing encoder.onnx or decode.onnx sub-graphs.".to_string());
                }
            }

            SttFamily::Paraformer | SttFamily::Zipformer => {
                let has_encoder = onnx_files.iter().any(|f| f.contains("encoder") || f.contains("model"));
                if !has_encoder {
                    return Err("Paraformer/Zipformer Contract Violation: Missing encoder.onnx transducer graph.".to_string());
                }
            }

            SttFamily::Wav2Vec2 => {
                let has_model = onnx_files.iter().any(|f| f.contains("model") || f.contains("wav2vec2"));
                if !has_model {
                    return Err("Wav2Vec2 Contract Violation: Missing primary model.onnx graph.".to_string());
                }
            }

            _ => {
                if onnx_files.is_empty() && gguf_bin_files.is_empty() {
                    return Err("Generic STT Contract Violation: Missing primary model weight files.".to_string());
                }
            }
        }

        Ok(())
    }
}
