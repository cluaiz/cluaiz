use anyhow::{anyhow, Result};
use ort::session::Session;
use super::reading::{self, Chunk};
#[cfg(test)]
use super::reading::chunk_tts_text;

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
        tracing::debug!("⚠️ [TTS Router] Output signal amplitude is near noise floor ({:.6}). Preserving raw PCM without scaling to prevent noise amplification.", max_amplitude);
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
    let raw_text_input = if prompt.contains("[TEXT_INPUT:") {
        extract_parameter(prompt, "TEXT_INPUT")
            .ok_or_else(|| anyhow!("Malformed legacy TTS text envelope"))?
    } else {
        extract_clean_text_prompt(prompt)
            .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?
    };
    // Only the legacy envelope prefix can select a voice; user text is never metadata.
    let voice = prompt.find("[TEXT_INPUT")
        .and_then(|start| extract_parameter(&prompt[..start], "voice"));
    route_tts_input(engine, session, &raw_text_input, voice.as_deref(), 1.0, None)
}

/// Synthesize actual text without interpreting transport tags as user content.
pub fn route_tts_text(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    raw_text_input: &str,
) -> Result<String> {
    route_tts_input(engine, session, raw_text_input, None, 1.0, None)
}

/// Receives each part of the audio as soon as it is final, as a WAV data URI; false stops sending.
pub type AudioSink<'a> = &'a mut (dyn FnMut(String) -> bool + Send);

/// Synthesize text with a requested voice (None: by language) and speaking rate (1.0 = normal).
/// With `on_audio`, Kokoro and Piper send each phrase while the next one is synthesized; the
/// returned WAV is the whole text either way.
pub fn route_tts_with(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    raw_text_input: &str,
    voice: Option<&str>,
    speed: f32,
    on_audio: Option<AudioSink<'_>>,
) -> Result<String> {
    route_tts_input(engine, session, raw_text_input, voice, speed, on_audio)
}

/// Sends the part of the audio that later phrases can no longer change. Joining the next phrase
/// may trim the quiet end of the audio or blend its last samples, so that end is held back until
/// the next phrase (or the end of the text).
struct AudioStream<'a> {
    sink: Option<AudioSink<'a>>,
    sample_rate: usize,
    sent: usize,
}

/// Samples blended when two phrases join without a pause (reading::append_audio)
const JOIN_OVERLAP: usize = 120;
/// Below this level a sample may count as the silence a later join trims
const QUIET_LEVEL: f32 = 0.005;

impl<'a> AudioStream<'a> {
    fn new(sink: Option<AudioSink<'a>>, sample_rate: usize) -> Self {
        Self { sink, sample_rate, sent: 0 }
    }

    fn settled(&mut self, all: &[f32]) {
        let quiet_end = all.iter().rev().take_while(|v| v.abs() <= QUIET_LEVEL).count();
        self.send(all, all.len().saturating_sub(quiet_end + JOIN_OVERLAP));
    }

    fn finish(&mut self, all: &[f32]) {
        self.send(all, all.len());
    }

    fn send(&mut self, all: &[f32], end: usize) {
        let Some(sink) = self.sink.as_mut() else {
            return;
        };
        if end <= self.sent {
            return;
        }
        // Each part is its own small WAV; the level is clamped, not normalised (the whole text's
        // peak is not known yet)
        let part: Vec<f32> = all[self.sent..end].iter().map(|v| v.clamp(-0.95, 0.95)).collect();
        let wav = super::vocoder::NativeVocoder::new(self.sample_rate, 1920, 480, 80).encode_wav_bytes(&part);
        use base64::Engine;
        let uri = format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(&wav));
        if !sink(uri) {
            self.sink = None; // the client left: stop sending parts
        }
        self.sent = end;
    }
}

fn route_tts_input(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    raw_text_input: &str,
    requested_voice: Option<&str>,
    speed: f32,
    on_audio: Option<AudioSink<'_>>,
) -> Result<String> {
    if raw_text_input.trim().is_empty() {
        return Err(anyhow!("Text-to-Speech requires non-empty text"));
    }

    let model_dir = engine.model_dir.as_deref()
        .ok_or_else(|| anyhow!("TTS model directory not set"))?;
    let model_language = super::g2p::get_model_language(model_dir);
    let voice_lang = requested_voice.and_then(super::languages::voice_language);
    let base_language = voice_lang.unwrap_or(&model_language);
    let base_code = base_language.split(['-', '_']).next().unwrap_or(base_language);
    let allow_romanized = voice_lang.is_none()
        || base_code.eq_ignore_ascii_case("en")
        || super::rules_g2p::TRANSLITERATION_FILES
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(base_code));
    // Hindi typed in Latin letters (and other romanized languages) is read in its own script
    let romanized = if allow_romanized {
        super::romanized::to_native_script(raw_text_input, base_language)
    } else {
        std::borrow::Cow::Borrowed(raw_text_input)
    };
    let raw_text_input = romanized.as_ref();
    let language = super::rules_g2p::script_language(raw_text_input, base_language)
        .or_else(|| super::languages::script_language(raw_text_input))
        .map(str::to_owned)
        .unwrap_or_else(|| base_language.to_owned());
    let text_chunks = reading::prepare(raw_text_input, &language)?;
    let normalized_text = text_chunks.iter().map(|chunk| chunk.text.as_str()).collect::<Vec<_>>().join(" ");
    let raw_text_input = normalized_text.as_str();
    tracing::debug!(
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
    tracing::debug!(
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
    tracing::debug!(
        "🎙️ [TTS Router] Dynamically Resolved Target Sample Rate: {} Hz",
        sample_rate
    );

    let vocoder_wav = super::vocoder::NativeVocoder::new(sample_rate, 1920, 480, 80);

    match detected_family {
        super::family_adapter::TtsFamily::VitsPiper => {
            handle_vits_piper(engine, session, &text_chunks, &manifest, sample_rate, speed, on_audio)
        }
        super::family_adapter::TtsFamily::Kokoro => {
            handle_kokoro(engine, session, &text_chunks, sample_rate, requested_voice, speed, on_audio)
        }
        super::family_adapter::TtsFamily::Matcha => {
            let pcm = super::families::matcha::execute(engine, session, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::CosyVoice => {
            let pcm = super::families::cosyvoice::execute(engine, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Supertonic => {
            let pcm = super::families::supertonic::execute(engine, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Audio8 => {
            let pcm = super::families::audio8::execute(engine, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::Chatterbox => {
            let pcm = super::families::chatterbox::execute(engine, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::OmniVoice => {
            let pcm = super::families::omnivoice::execute(engine, raw_text_input)?;
            let sanitized_pcm = sanitize_audio_pcm(pcm)?;
            let wav_bytes = vocoder_wav.encode_wav_bytes(&sanitized_pcm);
            use base64::Engine;
            Ok(format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
            ))
        }
        super::family_adapter::TtsFamily::GenericOnnx => {
            tracing::debug!("⚠️ [TTS Router] Unknown family detected. Attempting VITS/Piper handler as fallback.");
            handle_vits_piper(engine, session, &text_chunks, &manifest, sample_rate, speed, on_audio)
        }
    }
}

/// VITS/Piper TTS Handler — uses PhonemeMap for proper tokenization
fn handle_vits_piper(
    engine: &crate::engine::OnnxEngine,
    session: &mut Session,
    text_chunks: &[Chunk],
    manifest: &super::manifest_loader::TtsModelManifest,
    sample_rate: usize,
    speed: f32,
    on_audio: Option<AudioSink<'_>>,
) -> Result<String> {
    let mut stream = AudioStream::new(on_audio, sample_rate);
    tracing::debug!(
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
    // Piper's length scale is the duration of each phoneme: a faster rate shortens it
    let length_scale = manifest.length_scale.unwrap_or(1.0) / speed;
    let noise_w = manifest.noise_scale_w.unwrap_or(0.8);

    let mut pause_before_ms = 0;

    for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
        let trimmed_chunk = text_chunk.text.trim();
        if trimmed_chunk.is_empty() {
            continue;
        }

        let processed_text = super::g2p::process_text_for_family(
            trimmed_chunk,
            &super::family_adapter::TtsFamily::VitsPiper,
            model_dir,
        );

        let token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            let ids = pmap.text_to_ids(&processed_text);
            pmap.require_phonemes(&ids)?;
            ids
        } else {
            return Err(anyhow!("PackageContractException: Piper/VITS model missing required 'tokens.txt' or 'phoneme_id_map' in model directory {:?}.", model_dir));
        };

        if !token_ids.is_empty() {
            tracing::debug!(
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

            reading::append_audio(&mut all_pcm_samples, &chunk_pcm, sample_rate, pause_before_ms)?;
            stream.settled(&all_pcm_samples);
            pause_before_ms = text_chunk.pause_after_ms;
        }
    }

    stream.finish(&all_pcm_samples);
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
    text_chunks: &[Chunk],
    sample_rate: usize,
    requested_voice: Option<&str>,
    speed: f32,
    on_audio: Option<AudioSink<'_>>,
) -> Result<String> {
    let mut stream = AudioStream::new(on_audio, sample_rate);
    tracing::debug!(
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

    let full_text = text_chunks.iter().map(|chunk| chunk.text.as_str()).collect::<Vec<_>>().join(" ");
    // A requested voice, else one whose name says it speaks the text's language (its script, or the
    // model's language), else the first voice (data/languages.json)
    let text_lang = super::languages::script_language(&full_text)
        .or_else(|| requested_voice.and_then(super::languages::voice_language))
        .map(str::to_owned)
        .unwrap_or_else(|| super::g2p::get_model_language(model_dir));
    let selected_voice = requested_voice.map(str::to_owned)
        .or_else(|| super::languages::voice_for_language(model_dir, &text_lang))
        .unwrap_or_default();
    let active_lang = super::languages::voice_language(&selected_voice).unwrap_or(&text_lang);
    tracing::info!("🎙️ [Kokoro Handler] Voice '{selected_voice}' for language '{active_lang}'");

    // Load style vector from voices/ directory
    let style_vector = super::families::kokoro::load_style_vector(model_dir, &selected_voice)
        .or_else(|_| super::families::kokoro::load_first_available_voice(model_dir))
        .map_err(|e| anyhow!("Kokoro style embedding load failed for voice '{}': {}. Place a voice .bin file in the model's voices/ directory.", selected_voice, e))?;

    let mut pause_before_ms = 0;
    for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
        let processed_text = super::g2p::process_text_for_family_with_lang(
            text_chunk.text.trim(),
            &super::family_adapter::TtsFamily::Kokoro,
            model_dir,
            Some(active_lang),
        );
        let mut token_ids: Vec<i64> = if let Some(ref pmap) = phoneme_map {
            let ids = pmap.text_to_ids_no_pad(&processed_text);
            pmap.require_phonemes(&ids)?;
            ids
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

        tracing::debug!(
            "🎙️ [Kokoro Handler] Chunk {}/{}: {} chars → {} tokens",
            chunk_idx + 1,
            text_chunks.len(),
            text_chunk.text.len(),
            token_ids.len()
        );

        let chunk_pcm =
            super::families::kokoro::execute(session, &token_ids, &style_vector, speed)?;

        reading::append_audio(&mut all_pcm_samples, &chunk_pcm, sample_rate, pause_before_ms)?;
        stream.settled(&all_pcm_samples);
        pause_before_ms = text_chunk.pause_after_ms;
    }

    stream.finish(&all_pcm_samples);
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
    let mut depth = 0usize;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(rest[..i].trim().to_owned()),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_text_preserves_nested_bracket_content() {
        let data: serde_json::Value = serde_json::from_str(include_str!("data/reading_tests.json")).unwrap();
        for case in data["legacy"].as_array().unwrap() {
            assert_eq!(extract_parameter(case["input"].as_str().unwrap(), "TEXT_INPUT").as_deref(), case["expected"].as_str());
        }
    }

    #[test]
    fn test_reading_script_selection_preserves_same_script_model_language() {
        let data: serde_json::Value = serde_json::from_str(include_str!("data/reading_tests.json")).unwrap();
        for case in data["languages"].as_array().unwrap() {
            assert_eq!(super::super::rules_g2p::script_language(case["input"].as_str().unwrap(), case["model"].as_str().unwrap()), case["expected"].as_str());
        }
    }

    fn fixture() -> (String, usize, String) {
        let data: serde_json::Value = serde_json::from_str(include_str!("data/pronunciation_tests.json")).unwrap();
        let case = &data["chunking"];
        let unit = case["unit"].as_str().unwrap();
        let text = vec![unit; case["repeat"].as_u64().unwrap() as usize].join(" ");
        (text, case["max_bytes"].as_u64().unwrap() as usize, case["language"].as_str().unwrap().to_owned())
    }

    #[test]
    fn test_chunk_long_sentence_keeps_whole_words_and_order() {
        let (text, limit, _) = fixture();
        let chunks = chunk_tts_text(&text, limit);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| !chunk.is_empty() && chunk.len() <= limit));
        assert_eq!(chunks.join(" "), text);
        let word = text.split_whitespace().next().unwrap();
        assert_eq!(chunk_tts_text(word, 1), vec![word]);
    }

    #[test]
    fn test_voice_language_routes_latin_and_non_english_g2p_correctly() {
        let dir = std::env::temp_dir().join(format!("cluaiz-voice-g2p-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Minimal Kokoro vocab with misaki markers A, I, O, W, Y and affricate ʧ
        std::fs::write(
            dir.join("tokenizer.json"),
            r#"{"vocab":{"$":0,"A":1,"I":2,"O":3,"W":4,"Y":5,"ʧ":6,"a":7,"o":8,"l":9,"n":10,"m":11,"ə":12,"s":13,"t":14,"e":15,"ː":16}}"#,
        )
        .unwrap();

        let it_lang = super::super::languages::voice_language("if_sara").unwrap();
        assert_eq!(it_lang, "it");
        let it_ipa = super::super::g2p::process_text_for_family_with_lang(
            "ciao",
            &super::super::family_adapter::TtsFamily::Kokoro,
            &dir,
            Some(it_lang),
        );
        // Italian G2P ("ciao" -> "tʃao") converted via misaki-espeak ("tʃ" -> "ʧ")
        assert_eq!(it_ipa, "ʧao");

        let hi_lang = super::super::languages::voice_language("hf_alpha").unwrap();
        assert_eq!(hi_lang, "hi");
        let hi_ipa = super::super::g2p::process_text_for_family_with_lang(
            "नमस्ते",
            &super::super::family_adapter::TtsFamily::Kokoro,
            &dir,
            Some(hi_lang),
        );
        assert_eq!(hi_ipa, "nəməsteː");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_english_words_in_hindi_voice_text_keep_the_model_dictionary() {
        let dir = std::env::temp_dir().join(format!("cluaiz-voice-mixed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("tokenizer.json"),
            r#"{"vocab":{"$":0,"A":1,"I":2,"O":3,"W":4,"Y":5,"ʧ":6,"a":7,"o":8,"l":9,"n":10,"m":11,"ə":12,"s":13,"t":14,"e":15,"ː":16,"h":17}}"#,
        )
        .unwrap();
        // A model language with no downloadable dictionary, so only the model's own lexicon counts
        std::fs::write(dir.join("config.json"), r#"{"espeak":{"voice":"qx"}}"#).unwrap();
        std::fs::write(dir.join("lexicon.txt"), "hello həlO\n").unwrap();

        let kokoro = &super::super::family_adapter::TtsFamily::Kokoro;
        let hindi_only = super::super::g2p::process_text_for_family_with_lang("नमस्ते", kokoro, &dir, Some("hi"));
        let mixed = super::super::g2p::process_text_for_family_with_lang("hello नमस्ते", kokoro, &dir, Some("hi"));
        assert_eq!(mixed, format!("həlO {hindi_only}"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "requires CLUAIZ_TTS_TEST_MODEL pointing to staged genuine model metadata"]
    fn test_long_sentence_model_token_counts() {
        let (text, limit, language) = fixture();
        let dir = std::path::PathBuf::from(std::env::var("CLUAIZ_TTS_TEST_MODEL").unwrap());
        let map = super::super::phoneme_map::PhonemeMap::from_model_dir(&dir).unwrap();
        let converter = super::super::phoneme_sets::converter_for(&dir, &language);
        let count = |text: &str| {
            let ipa = text.split_whitespace().map(|word| super::super::rules_g2p::word_to_ipa(word, &language).unwrap().0).collect::<Vec<_>>().join(" ");
            let converted = converter.as_ref().map_or(ipa.clone(), |c| c.convert(&ipa));
            map.text_to_ids_no_pad(&converted).len()
        };
        let chunks = chunk_tts_text(&text, limit);
        let counts: Vec<_> = chunks.iter().map(|chunk| count(chunk)).collect();
        println!("unchunked_tokens={} chunk_count={} chunk_tokens={counts:?}", count(&text), chunks.len());
        assert!(chunks.len() > 1);
    }
}

