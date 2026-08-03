use anyhow::{anyhow, Result};
use ort::session::Session;

/// Universal PCM Audio Output Sanitizer
/// Validates generated PCM audio float buffer for finite values and normalizes peak amplitude
pub fn sanitize_audio_pcm(mut pcm: Vec<f32>) -> Result<Vec<f32>> {
    if pcm.is_empty() {
        return Err(anyhow!("AudioSanitizationException: Generated PCM audio buffer is completely empty (0 samples)."));
    }

    let mut max_amplitude = 0.0f32;
    for &sample in &pcm {
        if sample.is_nan() || sample.is_infinite() {
            return Err(anyhow!("AudioSanitizationException: Generated audio buffer contains NaN or Infinite floats. Aborting synthesis to prevent speaker damage/buzzing noise."));
        }
        if sample.abs() > max_amplitude {
            max_amplitude = sample.abs();
        }
    }

    if max_amplitude > 1e-5 {
        let target_peak = 0.85f32;
        let scale = target_peak / max_amplitude;
        for s in &mut pcm {
            *s = (*s * scale).clamp(-0.95, 0.95);
        }
    }

    Ok(pcm)
}

/// Universal TTS Router
/// Inspects the exact input/output tensor signatures of the ONNX graph
/// to determine which TTS strategy to use (VITS, Flow-Matching, Vocoder, etc.)
pub fn route_tts_inference(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    prompt: &str,
    tokenizer: Option<&tokenizers::Tokenizer>,
) -> Result<String> {

    let raw_text_input = extract_parameter(prompt, "TEXT_INPUT")
        .or_else(|| extract_clean_text_prompt(prompt))
        .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?;

    let text_chunks = chunk_tts_text(&raw_text_input, 140);
    eprintln!("==================================================");
    eprintln!("🚀 [CLUAIZ ENGINE] EXECUTING TTS ROUTER DISPATCH 🚀");
    eprintln!("==================================================");
    eprintln!("🎙️ [TTS Router] Original Length: {} chars | Total Sentences/Chunks: {}", raw_text_input.len(), text_chunks.len());

    let dummy_dir = engine.model_dir.as_deref().unwrap_or_else(|| std::path::Path::new("."));
    let sessions_pairs = [("primary", &*session)];
    let detected_family = super::family_adapter::FamilyAdapter::detect_family(dummy_dir, &sessions_pairs);
    eprintln!("🎯 [TTS Router] FamilyAdapter Detected Model Family: {:?}", detected_family);

    // Asset Inventory Gate Check before running session inference
    super::family_adapter::FamilyAdapter::validate_package_inventory(&detected_family, dummy_dir)?;

    match detected_family {
        super::family_adapter::TtsFamily::VitsPiper => {
            handle_vits_piper(engine, session, &text_chunks)
        }
        super::family_adapter::TtsFamily::Kokoro => {
            handle_kokoro(engine, session, &text_chunks)
        }
        super::family_adapter::TtsFamily::CosyVoiceMatcha => {
            handle_flow_estimator(engine, session, &text_chunks, tokenizer)
        }
        super::family_adapter::TtsFamily::Supertonic => {
            Err(anyhow!("Supertonic diffusion TTS is not yet fully implemented. Missing: duration predictor stage, diffusion denoising loop. Only text_encoder + vocoder stages exist."))
        }
        super::family_adapter::TtsFamily::Audio8
        | super::family_adapter::TtsFamily::Chatterbox
        | super::family_adapter::TtsFamily::OmniVoice => {
            Err(anyhow!("TTS family {:?} is not yet implemented. Please use a VitsPiper or Kokoro model.", detected_family))
        }
        super::family_adapter::TtsFamily::GenericOnnx => {
            // Last resort: try VITS handler since most single-ONNX models are VITS-based
            eprintln!("⚠️ [TTS Router] Unknown family detected. Attempting VITS/Piper handler as fallback.");
            handle_vits_piper(engine, session, &text_chunks)
        }
    }
}

fn handle_flow_estimator(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    text_chunks: &[String],
    tokenizer: Option<&tokenizers::Tokenizer>,
) -> Result<String> {
    eprintln!("🎙️ [TTS Router] Flow Estimator Graph Detected.");
    let vocoder = super::vocoder::NativeVocoder::default();
    let mut all_pcm_samples: Vec<f32> = Vec::new();

    let model_dir = engine.model_dir.as_deref().unwrap_or_else(|| std::path::Path::new("."));
    for (idx, text_chunk) in text_chunks.iter().enumerate() {
        let mut clean_chunk = text_chunk.clone();
        while let Some(start) = clean_chunk.find('[') {
            if let Some(end) = clean_chunk[start..].find(']') {
                clean_chunk.replace_range(start..=end, "");
            } else {
                break;
            }
        }
        let processed_text = super::g2p::process_text_for_family(clean_chunk.trim(), &super::family_adapter::TtsFamily::CosyVoiceMatcha, model_dir);
        let seq_len = (processed_text.bytes().count() * 4).clamp(50, 200);
        match super::flow_matching::FlowMatchingSampler::sample_mel_features_with_text(session, engine, tokenizer, &processed_text, seq_len, 10) {
            Ok(mel_data) => {
                let chunk_pcm = if let Some(voc_arc) = &engine.vocoder_session {
                    if let Ok(mut voc_sess) = voc_arc.lock() {
                        super::neural_vocoder::synthesize_mel_to_pcm(&mut voc_sess, &mel_data, seq_len)
                            .map_err(|e| anyhow!("Neural vocoder synthesis failed: {}. Package requires valid neural vocoder graph.", e))?
                    } else {
                        return Err(anyhow!("Neural Vocoder session is locked or unavailable. Required for Flow-Matching TTS."));
                    }
                } else {
                    return Err(anyhow!("PackageContractException: Neural Vocoder graph (vocoder.onnx / generator.onnx) missing from model directory."));
                };

                all_pcm_samples.extend_from_slice(&chunk_pcm);
                all_pcm_samples.resize(all_pcm_samples.len() + 1200, 0.0f32);
                eprintln!("🎙️ [TTS Router] Flow Chunk {}/{} complete", idx + 1, text_chunks.len());
            },
            Err(e) => {
                return Err(anyhow!("Flow Matching failed for chunk {}: {}", idx + 1, e));
            }
        }
    }

    let sanitized_pcm = sanitize_audio_pcm(all_pcm_samples)?;
    let wav_bytes = vocoder.encode_wav_bytes(&sanitized_pcm);
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
    Ok(format!("data:audio/wav;base64,{}", b64))
}

/// VITS/Piper TTS Handler — uses PhonemeMap for proper tokenization
fn handle_vits_piper(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    text_chunks: &[String],
) -> Result<String> {
    eprintln!("🎙️ [TTS Router] VITS/Piper Handler Activated.");
    let vocoder_wav = super::vocoder::NativeVocoder::default();
    let mut all_pcm_samples: Vec<f32> = Vec::new();

    // Load PhonemeMap from model directory config files
    let model_dir = engine.model_dir.as_deref()
        .ok_or_else(|| anyhow!("Model directory not set. Cannot load phoneme_id_map for VITS tokenization."))?;

    let phoneme_map = super::phoneme_map::PhonemeMap::from_model_dir(model_dir);

    for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
        let mut clean_chunk = text_chunk.clone();
        while let Some(start) = clean_chunk.find('[') {
            if let Some(end) = clean_chunk[start..].find(']') {
                clean_chunk.replace_range(start..=end, "");
            } else {
                break;
            }
        }
        let processed_text = super::g2p::process_text_for_family(clean_chunk.trim(), &super::family_adapter::TtsFamily::VitsPiper, model_dir);
        // Tokenize using PhonemeMap if available, otherwise fall back to character bytes
        let token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            pmap.text_to_ids(&processed_text)
        } else {
            eprintln!("⚠️ [VITS Handler] No phoneme_id_map found in model dir. Falling back to raw byte tokenization.");
            // Fallback: BOS + char bytes + EOS (matches basic VITS input contract)
            let mut ids: Vec<i64> = vec![1]; // BOS
            for b in text_chunk.bytes() {
                ids.push(b as i64);
                ids.push(0); // PAD between chars
            }
            ids.push(2); // EOS
            ids
        };

        if token_ids.is_empty() {
            continue;
        }

        eprintln!("🎙️ [VITS Handler] Chunk {}/{}: {} chars → {} tokens",
            chunk_idx + 1, text_chunks.len(), text_chunk.len(), token_ids.len());

        let chunk_pcm = super::vits_handler::execute_vits(
            session,
            &token_ids,
            0.667, // noise_scale
            1.0,   // length_scale (speaking rate)
            0.8,   // noise_w
            None,  // speaker_id
        )?;

        all_pcm_samples.extend_from_slice(&chunk_pcm);
        // Add small silence gap between chunks
        all_pcm_samples.resize(all_pcm_samples.len() + 1200, 0.0f32);
    }

    let sanitized_pcm = sanitize_audio_pcm(all_pcm_samples)?;
    let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
    Ok(format!("data:audio/wav;base64,{}", b64))
}

/// Kokoro TTS Handler — uses style embeddings from voices/*.bin
fn handle_kokoro(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    text_chunks: &[String],
) -> Result<String> {
    eprintln!("🎙️ [TTS Router] Kokoro Handler Activated.");
    let vocoder_wav = super::vocoder::NativeVocoder::default();
    let mut all_pcm_samples: Vec<f32> = Vec::new();

    let model_dir = engine.model_dir.as_deref()
        .ok_or_else(|| anyhow!("Model directory not set. Cannot load Kokoro assets."))?;

    let phoneme_map = super::phoneme_map::PhonemeMap::from_model_dir(model_dir);

    let full_text = text_chunks.join(" ");
    let selected_voice = extract_parameter(&full_text, "voice").unwrap_or_else(|| "af_heart".to_string());

    // Load style vector from voices/ directory
    let style_vector = super::kokoro_handler::load_style_vector(model_dir, &selected_voice)
        .or_else(|_| super::kokoro_handler::load_style_vector(model_dir, "default"))
        .or_else(|_| super::kokoro_handler::load_first_available_voice(model_dir))
        .map_err(|e| anyhow!("Kokoro style embedding load failed for voice '{}': {}. Place a voice .bin file in the model's voices/ directory.", selected_voice, e))?;

    for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
        let mut clean_chunk = text_chunk.clone();
        while let Some(start) = clean_chunk.find('[') {
            if let Some(end) = clean_chunk[start..].find(']') {
                clean_chunk.replace_range(start..=end, "");
            } else {
                break;
            }
        }
        let processed_text = super::g2p::process_text_for_family(clean_chunk.trim(), &super::family_adapter::TtsFamily::Kokoro, model_dir);
        let token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            pmap.text_to_ids_no_pad(&processed_text)
        } else {
            // Kokoro uses phoneme tokens — raw bytes won't work well
            eprintln!("⚠️ [Kokoro Handler] No phoneme_id_map found. Using raw byte fallback.");
            text_chunk.bytes().map(|b| b as i64).collect()
        };

        if token_ids.is_empty() {
            continue;
        }

        eprintln!("🎙️ [Kokoro Handler] Chunk {}/{}: {} chars → {} tokens",
            chunk_idx + 1, text_chunks.len(), text_chunk.len(), token_ids.len());

        let chunk_pcm = super::kokoro_handler::execute_kokoro(
            session,
            &token_ids,
            &style_vector,
            1.0, // speed
        )?;

        all_pcm_samples.extend_from_slice(&chunk_pcm);
        all_pcm_samples.resize(all_pcm_samples.len() + 1200, 0.0f32);
    }

    let sanitized_pcm = sanitize_audio_pcm(all_pcm_samples)?;
    let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
    Ok(format!("data:audio/wav;base64,{}", b64))
}

// NOTE: handle_diffusion_tts (Supertonic) removed.
// It was producing garbage output (hardcoded 0.05 latent, missing duration predictor + diffusion loop).
// The router now returns a clear error for Supertonic family.

fn extract_parameter(prompt: &str, param_name: &str) -> Option<String> {
    let tag = format!("[{}:", param_name);
    let start = prompt.find(&tag)?;
    let rest = &prompt[start + tag.len()..];
    let end = rest.find(']')?;
    Some(rest[..end].trim().to_string())
}

fn extract_clean_text_prompt(prompt: &str) -> Option<String> {
    let clean = prompt
        .replace("[AUDIO_INPUT]", "")
        .replace("[TEXT_INPUT]", "")
        .trim()
        .to_string();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn chunk_tts_text(text: &str, max_chunk_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for sentence in text.split_inclusive(&['.', '!', '?', ';', '\n', ','][..]) {
        if current_chunk.len() + sentence.len() > max_chunk_len && !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
            current_chunk = String::new();
        }
        current_chunk.push_str(sentence);
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}
