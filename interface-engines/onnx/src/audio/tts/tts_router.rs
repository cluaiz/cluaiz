use anyhow::{anyhow, Result};
use ort::session::Session;

/// Universal PCM Audio Output Sanitizer
/// Validates generated PCM audio float buffer for finite values and normalizes peak amplitude cleanly
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

    // Only normalize peak gain if maximum signal exceeds meaningful audio threshold (> 0.01 RMS floor)
    // Prevents multiplying background neural floating-point noise into loud blowing air noise ("fhoor fhoor")
    if max_amplitude > 0.01f32 {
        let scale = 0.85f32 / max_amplitude;
        for s in &mut pcm {
            *s = (*s * scale).clamp(-0.95, 0.95);
        }
    } else {
        eprintln!("⚠️ [TTS Router] Output signal amplitude is near noise floor ({:.6}). Preserving raw PCM without scaling to prevent noise amplification.", max_amplitude);
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
    _tokenizer: Option<&tokenizers::Tokenizer>,
) -> Result<String> {
    let raw_text_input = extract_parameter(prompt, "TEXT_INPUT")
        .or_else(|| extract_clean_text_prompt(prompt))
        .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?;

    let text_chunks = chunk_tts_text(&raw_text_input, 140);
    eprintln!("==================================================");
    eprintln!("🚀 [CLUAIZ ENGINE] EXECUTING TTS ROUTER DISPATCH 🚀");
    eprintln!("==================================================");
    eprintln!(
        "🎙️ [TTS Router] Original Length: {} chars | Total Sentences/Chunks: {}",
        raw_text_input.len(),
        text_chunks.len()
    );

    let dummy_dir = engine
        .model_dir
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("."));
    let sessions_pairs = [("primary", &*session)];
    let detected_family =
        super::family_adapter::FamilyAdapter::detect_family(dummy_dir, &sessions_pairs);
    eprintln!(
        "🎯 [TTS Router] FamilyAdapter Detected Model Family: {:?}",
        detected_family
    );

    // Asset Inventory Gate Check before running session inference
    super::family_adapter::FamilyAdapter::validate_package_inventory(&detected_family, dummy_dir)?;

    // Dynamically parse ONNX session metadata & manifest assets
    let manifest = super::manifest_loader::TtsModelManifest::parse_from_session(session, dummy_dir);
    let sample_rate = manifest.sample_rate.unwrap_or(match detected_family {
        super::family_adapter::TtsFamily::VitsPiper | super::family_adapter::TtsFamily::Matcha => {
            22050
        }
        super::family_adapter::TtsFamily::Audio8 => 44100,
        _ => 24000,
    }) as usize;
    eprintln!(
        "🎙️ [TTS Router] Dynamically Resolved Target Sample Rate: {} Hz",
        sample_rate
    );

    let vocoder_wav = super::vocoder::NativeVocoder::new(sample_rate, 1920, 480, 80);

    match detected_family {
        super::family_adapter::TtsFamily::VitsPiper => {
            handle_vits_piper(engine, session, &text_chunks, &manifest, sample_rate)
        }
        super::family_adapter::TtsFamily::Kokoro => {
            handle_kokoro(engine, session, &text_chunks, sample_rate)
        }
        super::family_adapter::TtsFamily::Matcha => {
            let pcm = super::families::matcha::execute(engine, session, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::CosyVoice => {
            let pcm = super::families::cosyvoice::execute(engine, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Supertonic => {
            let pcm = super::families::supertonic::execute(engine, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Audio8 => {
            let pcm = super::families::audio8::execute(engine, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Chatterbox => {
            let pcm = super::families::chatterbox::execute(engine, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::OmniVoice => {
            let pcm = super::families::omnivoice::execute(engine, &raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::GenericOnnx => {
            eprintln!("⚠️ [TTS Router] Unknown family detected. Attempting VITS/Piper handler as fallback.");
            handle_vits_piper(engine, session, &text_chunks, &manifest, sample_rate)
        }
    }
}

fn append_pcm_with_crossfade(all_pcm: &mut Vec<f32>, chunk_pcm: &[f32]) {
    if chunk_pcm.is_empty() {
        return;
    }
    if all_pcm.is_empty() {
        all_pcm.extend_from_slice(chunk_pcm);
        return;
    }

    let crossfade_len = 120.min(all_pcm.len()).min(chunk_pcm.len());
    let start_idx = all_pcm.len() - crossfade_len;

    for i in 0..crossfade_len {
        let alpha = i as f32 / crossfade_len as f32;
        all_pcm[start_idx + i] = all_pcm[start_idx + i] * (1.0 - alpha) + chunk_pcm[i] * alpha;
    }

    all_pcm.extend_from_slice(&chunk_pcm[crossfade_len..]);
}

/// VITS/Piper TTS Handler — uses PhonemeMap for proper tokenization
fn handle_vits_piper(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    text_chunks: &[String],
    manifest: &super::manifest_loader::TtsModelManifest,
    sample_rate: usize,
) -> Result<String> {
    eprintln!(
        "🎙️ [TTS Router] VITS/Piper Handler Activated with sample_rate={}Hz.",
        sample_rate
    );
    let vocoder_wav = super::vocoder::NativeVocoder::new(sample_rate, 1920, 480, 80);
    let mut all_pcm_samples: Vec<f32> = Vec::new();

    // Load PhonemeMap from model directory config files
    let model_dir = engine.model_dir.as_deref().ok_or_else(|| {
        anyhow!("Model directory not set. Cannot load phoneme_id_map for VITS tokenization.")
    })?;

    let phoneme_map = super::phoneme_map::PhonemeMap::from_model_dir(model_dir);

    let noise_scale = manifest.noise_scale.unwrap_or(0.667);
    let length_scale = manifest.length_scale.unwrap_or(1.0);
    let noise_w = manifest.noise_scale_w.unwrap_or(0.8);

    let pause_samples = (sample_rate as f32 * 0.18) as usize; // 180ms natural sentence boundary pause

    for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
        let mut clean_chunk = text_chunk.clone();
        while let Some(start) = clean_chunk.find('[') {
            if let Some(end) = clean_chunk[start..].find(']') {
                clean_chunk.replace_range(start..=end, "");
            } else {
                break;
            }
        }
        let trimmed_chunk = clean_chunk.trim();
        if trimmed_chunk.is_empty() {
            continue;
        }

        let processed_text = super::g2p::process_text_for_family(
            trimmed_chunk,
            &super::family_adapter::TtsFamily::VitsPiper,
            model_dir,
        );

        let token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            pmap.text_to_ids(&processed_text)
        } else {
            return Err(anyhow!("PackageContractException: Piper/VITS model missing required 'tokens.txt' or 'phoneme_id_map' in model directory {:?}.", model_dir));
        };

        if !token_ids.is_empty() {
            eprintln!(
                "🎙️ [VITS Handler] Sentence Chunk {}/{}: {} chars → {} tokens",
                chunk_idx + 1,
                text_chunks.len(),
                trimmed_chunk.len(),
                token_ids.len()
            );

            let mut chunk_pcm = super::families::vits_piper::execute(
                session,
                &token_ids,
                noise_scale,
                length_scale,
                noise_w,
                None,
            )?;

            // Apply a brief 400-sample linear fade-out at the end of each chunk
            // to prevent click/static hiss artifacts at active vocoder boundaries.
            let fade_len = 400.min(chunk_pcm.len());
            if fade_len > 0 {
                let start_idx = chunk_pcm.len() - fade_len;
                for i in 0..fade_len {
                    let alpha = (fade_len - i) as f32 / fade_len as f32;
                    chunk_pcm[start_idx + i] *= alpha;
                }
            }

            if !all_pcm_samples.is_empty() {
                // Insert natural 180ms pause between sentence chunks
                all_pcm_samples.extend(std::iter::repeat(0.0f32).take(pause_samples));
            }
            all_pcm_samples.extend_from_slice(&chunk_pcm);
        }
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
    sample_rate: usize,
) -> Result<String> {
    eprintln!(
        "🎙️ [TTS Router] Kokoro Handler Activated with sample_rate={}Hz.",
        sample_rate
    );
    let vocoder_wav = super::vocoder::NativeVocoder::new(sample_rate, 1920, 480, 80);
    let mut all_pcm_samples: Vec<f32> = Vec::new();

    let model_dir = engine
        .model_dir
        .as_deref()
        .ok_or_else(|| anyhow!("Model directory not set. Cannot load Kokoro assets."))?;

    let phoneme_map = super::phoneme_map::PhonemeMap::from_model_dir(model_dir);

    let full_text = text_chunks.join(" ");
    let is_hindi_script = full_text
        .chars()
        .any(|c| ('\u{0900}'..='\u{097F}').contains(&c));
    let default_voice = if is_hindi_script {
        "hf_alpha"
    } else {
        "af_heart"
    };
    let selected_voice =
        extract_parameter(&full_text, "voice").unwrap_or_else(|| default_voice.to_string());

    // Load style vector from voices/ directory
    let style_vector = super::families::kokoro::load_style_vector(model_dir, &selected_voice)
        .or_else(|_| super::families::kokoro::load_style_vector(model_dir, "hm_omega"))
        .or_else(|_| super::families::kokoro::load_style_vector(model_dir, "af_heart"))
        .or_else(|_| super::families::kokoro::load_style_vector(model_dir, "default"))
        .or_else(|_| super::families::kokoro::load_first_available_voice(model_dir))
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
        let processed_text = super::g2p::process_text_for_family(
            clean_chunk.trim(),
            &super::family_adapter::TtsFamily::Kokoro,
            model_dir,
        );
        let mut token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            pmap.text_to_ids_no_pad(&processed_text)
        } else {
            return Err(anyhow!("PackageContractException: Kokoro model missing required 'tokens.txt' or 'phoneme_id_map' in model directory {:?}. Refusing to pass raw byte fallback to prevent garbled noise.", model_dir));
        };

        if token_ids.is_empty() {
            continue;
        }

        // Ensure 1 leading zero (BOS) and 1 trailing zero (EOS) padding token matching Sherpa-ONNX math
        if token_ids.first() != Some(&0) {
            token_ids.insert(0, 0);
        }
        if token_ids.last() != Some(&0) {
            token_ids.push(0);
        }

        eprintln!(
            "🎙️ [Kokoro Handler] Chunk {}/{}: {} chars → {} tokens",
            chunk_idx + 1,
            text_chunks.len(),
            text_chunk.len(),
            token_ids.len()
        );

        let chunk_pcm =
            super::families::kokoro::execute(session, &token_ids, &style_vector, 1.0f32)?;

        append_pcm_with_crossfade(&mut all_pcm_samples, &chunk_pcm);
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

    for sentence in text.split_inclusive(&['.', '!', '?', ';', '\n'][..]) {
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
