use ort::session::Session;
use std::collections::HashMap;
use std::path::Path;
use anyhow::{anyhow, Result};

/// Supported ONNX TTS Model Package Families
/// Each variant maps 1:1 to a unique pipeline topology from the taxonomy doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtsFamily {
    /// Family 1: Single-Stage End-to-End Variational Inference (Piper, VITS, MMS)
    VitsPiper,
    /// Family 2: Single-Stage Phoneme & Style Decoder (Kokoro-82M, Kitten-TTS)
    Kokoro,
    /// Family 3: 4-Stage Iterative Latent Denoising Diffusion (Supertonic 2/3)
    Supertonic,
    /// Family 4: 3-Stage Auto-Regressive Codec Transformer (Audio8)
    Audio8,
    /// Family 5: 2-Stage Acoustic Flow-Matching Pipeline (Matcha-TTS)
    /// Flow Estimator → Mel Spectrogram → HiFi-GAN → PCM
    Matcha,
    /// Family 6: Multi-Stage LLM & Acoustic Flow Synthesizer (CosyVoice 1/2/3)
    /// Speech LLM → Flow Matching → HiFT Vocoder → PCM
    CosyVoice,
    /// Family 7: Multi-Stage Semantic Generator & Neural Audio Codec (Chatterbox)
    Chatterbox,
    /// Family 8: ONNX Runtime GenAI Package (OmniVoice)
    OmniVoice,
    /// Fallback / Generic ONNX TTS Model
    GenericOnnx,
}

/// Package Inspector & Asset Inventory Gate Resolver
pub struct FamilyAdapter;

impl FamilyAdapter {
    /// Pre-Boot Asset Inventory Gate: Abort fast before allocating session RAM/GPU memory
    pub fn validate_package_inventory(family: &TtsFamily, model_dir: &Path) -> Result<()> {
        if !model_dir.exists() {
            return Err(anyhow!("Model directory does not exist: {:?}", model_dir));
        }

        match family {
            TtsFamily::Kokoro => {
                let voices_dir = model_dir.join("voices");
                let has_voices = voices_dir.exists() && voices_dir.is_dir();
                if !has_voices {
                    return Err(anyhow!(
                        "PackageContractException: Kokoro-82M model requires a 'voices/' directory containing style embeddings (e.g. af_heart.bin). Boot aborted."
                    ));
                }
            }
            TtsFamily::Supertonic => {
                let entries: Vec<String> = std::fs::read_dir(model_dir)
                    .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
                    .unwrap_or_default();

                let has_unified_primary = entries.iter().any(|e| e.contains("model.onnx") || e.contains("model_uint8") || e.contains("model_q4") || e.contains("model_int8") || e.contains("kokoro") || e.contains("supertonic.onnx"));
                
                if !has_unified_primary {
                    let required_files = [
                        "text_encoder",
                        "duration_predictor",
                        "vector_estimator",
                    ];
                    for req in &required_files {
                        let exists = entries.iter().any(|e| e.contains(req));
                        if !exists {
                            return Err(anyhow!(
                                "PackageContractException: Supertonic model missing required subcomponent ONNX graph '{}'. Boot aborted.",
                                req
                            ));
                        }
                    }
                }
            }
            TtsFamily::Matcha => {
                let entries: Vec<String> = std::fs::read_dir(model_dir)
                    .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
                    .unwrap_or_default();
                
                let has_acoustic = entries.iter().any(|e| e.contains("flow") || e.contains("matcha") || e.contains("estimator") || e.contains("fm_decoder"));
                if !has_acoustic {
                    return Err(anyhow!(
                        "PackageContractException: Matcha-TTS model requires an acoustic flow estimator graph (flow.decoder.estimator.onnx, fm_decoder, or matcha_acoustic.onnx). Boot aborted."
                    ));
                }
            }
            TtsFamily::CosyVoice => {
                let entries: Vec<String> = std::fs::read_dir(model_dir)
                    .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
                    .unwrap_or_default();
                
                let has_flow = entries.iter().any(|e| e.contains("flow") || e.contains("estimator"));
                let has_vocoder = entries.iter().any(|e| e.contains("hift") || e.contains("vocoder"));
                if !has_flow {
                    return Err(anyhow!(
                        "PackageContractException: CosyVoice model requires a flow decoder estimator graph (flow.decoder.estimator.onnx). Boot aborted."
                    ));
                }
                if !has_vocoder {
                    return Err(anyhow!(
                        "PackageContractException: CosyVoice model requires a HiFT vocoder graph (hift.onnx or vocoder.onnx). Boot aborted."
                    ));
                }
            }
            TtsFamily::Audio8 => {
                let has_slow_ar = model_dir.join("slow_ar_int4.onnx").exists() || model_dir.join("slow_ar.onnx").exists();
                if !has_slow_ar {
                    return Err(anyhow!(
                        "PackageContractException: Audio8 Codec-LM model missing required 'slow_ar.onnx' model graph. Boot aborted."
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Detect model family from manifest metadata, directory files, and ONNX session signatures
    pub fn detect_family(model_dir: &Path, sessions: &[(&str, &Session)]) -> TtsFamily {
        // 🎯 Priority 1: Check model_manifest.json or config.json for explicit tts_family tag
        let manifest_file = model_dir.join("model_manifest.json");
        let config_file = model_dir.join("config.json");

        let mut manifest_family: Option<String> = None;
        for path in &[&manifest_file, &config_file] {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(fam) = json.get("metadata").and_then(|m| m.get("tts_family")).and_then(|f| f.as_str()) {
                        manifest_family = Some(fam.to_lowercase());
                        break;
                    }
                    if let Some(fam) = json.get("tts_family").or_else(|| json.get("family")).and_then(|f| f.as_str()) {
                        manifest_family = Some(fam.to_lowercase());
                        break;
                    }
                }
            }
        }

        if let Some(ref fam) = manifest_family {
            match fam.as_str() {
                "kokoro" | "kokoro-v1" => return TtsFamily::Kokoro,
                "audio8" | "audio8-codec" => return TtsFamily::Audio8,
                "supertonic" | "supertonic-v3" | "luxtts" => return TtsFamily::Supertonic,
                "matcha" | "matcha-v1" => return TtsFamily::Matcha,
                "cosyvoice" | "cosyvoice_matcha" | "cosyvoice-v2" | "cosyvoice-v3" => return TtsFamily::CosyVoice,
                "vits_piper" | "vits" | "piper-vits" => return TtsFamily::VitsPiper,
                "chatterbox" => return TtsFamily::Chatterbox,
                "omnivoice" => return TtsFamily::OmniVoice,
                _ => {}
            }
        }

        for (name, session) in sessions {
            let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
            let lower_name = name.to_lowercase();

            if lower_name.contains("kokoro") || (input_names.iter().any(|n| n == "style") && input_names.iter().any(|x| x == "speed")) {
                return TtsFamily::Kokoro;
            }

            if input_names.iter().any(|n| n == "noisy_latent" || n == "current_step" || n == "style_ttl") {
                return TtsFamily::Supertonic;
            }

            // Distinguish CosyVoice from Matcha by checking for speech_llm/campplus presence
            if input_names.iter().any(|n| n == "mu" || n == "cond" || n == "spk_emb") && input_names.iter().any(|n| n == "estimator" || n == "flow") {
                // Check if model_dir contains speech_llm or campplus (CosyVoice signature)
                let dir_entries: Vec<String> = std::fs::read_dir(model_dir)
                    .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
                    .unwrap_or_default();
                let is_cosyvoice = dir_entries.iter().any(|e| e.contains("speech_llm") || e.contains("campplus") || e.contains("speech_tokenizer"));
                if is_cosyvoice {
                    return TtsFamily::CosyVoice;
                }
                return TtsFamily::Matcha;
            }

            if input_names.iter().any(|n| n == "input_lengths" || n == "scales" || n == "sid") {
                return TtsFamily::VitsPiper;
            }
        }

        let dir_name = model_dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if dir_name.contains("kokoro") {
            return TtsFamily::Kokoro;
        }
        if dir_name.contains("audio8") {
            return TtsFamily::Audio8;
        }
        if dir_name.contains("omnivoice") {
            return TtsFamily::OmniVoice;
        }
        if dir_name.contains("chatterbox") {
            return TtsFamily::Chatterbox;
        }
        if dir_name.contains("piper") || dir_name.contains("vits") {
            return TtsFamily::VitsPiper;
        }
        if dir_name.contains("matcha") || dir_name.contains("lux") {
            return TtsFamily::Matcha;
        }


        let mut has_duration_predictor = false;
        let mut has_text_encoder = false;
        let mut has_vector_estimator = false;
        let mut has_vocoder = false;

        if let Ok(entries) = std::fs::read_dir(model_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.ends_with(".onnx") {
                    if name.contains("matcha") {
                        return TtsFamily::Matcha;
                    }
                    if name.contains("cosyvoice") || name.contains("flow.decoder") {
                        // Check for CosyVoice-specific files in same dir
                        let has_cosyvoice_marker = std::fs::read_dir(model_dir)
                            .map(|rd| rd.flatten().any(|e| {
                                let n = e.file_name().to_string_lossy().to_lowercase();
                                n.contains("campplus") || n.contains("speech_llm") || n.contains("speech_tokenizer")
                            }))
                            .unwrap_or(false);
                        if has_cosyvoice_marker {
                            return TtsFamily::CosyVoice;
                        }
                        return TtsFamily::Matcha;
                    }
                    if name.contains("duration_predictor") || name.contains("dp.onnx") {
                        has_duration_predictor = true;
                    }
                    if name.contains("text_encoder") || name.contains("text_enc") {
                        has_text_encoder = true;
                    }
                    if name.contains("vector_estimator") {
                        has_vector_estimator = true;
                    }
                    if name.contains("vocoder") || name.contains("generator") || name.contains("hift") {
                        has_vocoder = true;
                    }
                }
            }
        }

        if has_vector_estimator || (has_text_encoder && has_duration_predictor) {
            return TtsFamily::Supertonic;
        }

        if has_vocoder {
            TtsFamily::Matcha
        } else {
            TtsFamily::VitsPiper
        }
    }

    /// Dynamically extract tensor input ranks and shapes from session.inputs()
    pub fn extract_input_ranks(session: &Session) -> HashMap<String, usize> {
        let mut ranks = HashMap::new();
        for input in session.inputs() {
            let name = input.name().to_string();
            let rank = if name.contains("step") {
                1
            } else if name.contains("mask") && (name.contains("text") || name.contains("latent")) {
                3
            } else if name.contains("style") || name.contains("voice") {
                2
            } else {
                3
            };
            ranks.insert(name, rank);
        }
        ranks
    }
}
