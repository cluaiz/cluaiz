use super::families::{self, SttEncoderKind};
use crate::engine::OnnxEngine;
use anyhow::{anyhow, Result};
use cluaiz_media::{
    audio::decode_audio,
    resample::{prepare_mono, resample_linear, PeakNormalization},
    source::read_local_or_inline,
};
use ort::value::Value;
use std::collections::HashMap;

impl OnnxEngine {
    pub fn execute_audio_graph(&self, prompt: &str) -> Result<String> {
        self.execute_audio_graph_streaming(prompt, None)
    }

    pub fn execute_audio_graph_streaming(
        &self,
        prompt: &str,
        callback: Option<&mut (dyn FnMut(String) -> bool + Send)>,
    ) -> Result<String> {
        let is_tts = prompt.contains("[TEXT_INPUT]")
            || (!prompt.contains("[AUDIO_INPUT") && !prompt.contains("encoder_hidden_states"));
        if is_tts {
            let session_arc = self
                .acquire_session()
                .map_err(|e| anyhow!("Failed to acquire audio TTS session: {}", e))?;
            let mut session_guard = session_arc
                .lock()
                .map_err(|_| anyhow!("Failed to lock ONNX audio TTS session mutex"))?;
            super::super::tts::route_tts_inference(
                self,
                &mut session_guard,
                prompt,
                self.tokenizer.as_deref(),
            )
        } else {
            self.execute_stt_graph_streaming(prompt, callback)
        }
    }

    pub fn execute_stt_graph(&self, prompt: &str) -> Result<String> {
        self.execute_stt_graph_streaming(prompt, None)
    }

    pub fn execute_stt_graph_streaming(
        &self,
        prompt: &str,
        callback: Option<&mut (dyn FnMut(String) -> bool + Send)>,
    ) -> Result<String> {
        let audio_path = extract_audio_path(prompt)
            .ok_or_else(|| anyhow!("No [AUDIO_INPUT: ...] tag found in prompt"))?;
        let limits = cluaiz_core::Settings::current().media.limits;
        let bytes = read_local_or_inline(&audio_path, limits, |_| Ok(()))?;
        let audio = decode_audio(bytes, limits)?;
        let options = crate::media::SttOptions {
            language: extract_parameter(prompt, "LANGUAGE"),
            languages: extract_parameter(prompt, "LANGUAGES")
                .map(|list| list.split(',').map(|code| code.trim().to_owned()).collect())
                .unwrap_or_default(),
            translate: extract_parameter(prompt, "TRANSLATE_TO")
                .is_some_and(|value| !value.is_empty() && value != "false"),
            ..Default::default()
        };
        self.execute_stt_prepared(&audio, &options, limits, callback)
    }

    /// Consume shared PCM directly; HTTP/file decoding belongs to the caller.
    ///
    /// Audio longer than the model's window (Whisper: 30 s) is split at its quietest moment near
    /// each window's end and the windows are transcribed in order, as whisper.cpp and OpenAI do.
    /// Without an explicit `language`, each window's language is identified on its own (among the
    /// speaker's `languages` when given), so a recording that changes language is followed.
    pub fn execute_stt_prepared(
        &self,
        audio: &cluaiz_core::media::DecodedAudio,
        options: &crate::media::SttOptions,
        limits: cluaiz_core::media_limits::MediaLimits,
        mut callback: Option<&mut (dyn FnMut(String) -> bool + Send)>,
    ) -> Result<String> {
        let session_arc = self
            .acquire_session()
            .map_err(|e| anyhow!("Failed to acquire audio session: {}", e))?;

        let mut decoder = session_arc
            .lock()
            .map_err(|_| anyhow!("Failed to lock ONNX audio decoder session mutex"))?;
        if !crate::media::is_uncached_decoder(&decoder) {
            return Err(anyhow!("STT requires a validated uncached decoder; unsupported cache inputs are not synthesized"));
        }
        // The decoder that continues from cached keys/values (one token per step), when the
        // package has one; without it every step re-reads the whole transcript so far
        let cached_arc = self.cached_decoder_session.clone();
        let mut cached = match &cached_arc {
            Some(arc) => Some(
                arc.lock()
                    .map_err(|_| anyhow!("Failed to lock ONNX cached decoder session mutex"))?,
            ),
            None => None,
        };

        let t_start = std::time::Instant::now();

        let req_lang = options.language.as_deref().unwrap_or("auto");
        let encoder_arc = self
            .encoder_session
            .as_ref()
            .ok_or_else(|| anyhow!("STT encoder missing"))?;
        let mut encoder = encoder_arc
            .lock()
            .map_err(|_| anyhow!("STT encoder mutex poisoned"))?;
        let encoder_input = families::encoder_input_name(&encoder)
            .ok_or_else(|| anyhow!("Unsupported STT encoder input signature"))?
            .to_owned();
        let profile = families::resolve_stt_profile(self, &encoder_input)?;
        profile.validate_requested_language(req_lang)?;

        let t_pcm = std::time::Instant::now();
        let mono = prepare_mono(audio, PeakNormalization::LimitToUnit, limits)?;
        let pcm = resample_linear(&mono, profile.sample_rate, limits)?;
        drop(mono);
        let pcm_samples = pcm.samples();
        let windows = families::split_windows(pcm_samples, profile.sample_rate, profile.max_samples);
        tracing::debug!(
            "⏱️ [BENCHMARK STT] Step A - PCM Load Time: {:?} (Audio Duration: {:.2}s, Samples: {}, Windows: {})",
            t_pcm.elapsed(),
            pcm_samples.len() as f32 / profile.sample_rate as f32,
            pcm_samples.len(),
            windows.len()
        );

        let tokenizer = self
            .tokenizer
            .as_deref()
            .ok_or_else(|| anyhow!("STT tokenizer missing"))?;
        let mut texts: Vec<String> = Vec::new();
        for window in windows {
            let samples = &pcm_samples[window];
            let t_enc = std::time::Instant::now();
            let hidden_states = match profile.encoder_kind {
                SttEncoderKind::MelSpectrogram { .. } => {
                    families::whisper::encode_whisper_features(&mut encoder, samples, &profile, limits)?
                }
                SttEncoderKind::RawWaveform { .. } => {
                    families::moonshine::encode_moonshine_waveform(&mut encoder, samples, &profile, limits)?
                }
            };
            tracing::debug!(
                "⏱️ [BENCHMARK STT] Step C - Encoder ONNX Model Inference Time: {:?}",
                t_enc.elapsed()
            );
            let encoder_val: Value = Value::from_array(hidden_states)
                .map(|v| v.into())
                .map_err(|e| anyhow!("Failed to construct encoder hidden states tensor: {}", e))?;

            let prompt = self.decoder_prompt(&mut decoder, &encoder_val, &profile, options)?;
            let window_secs = samples.len() as f32 / profile.sample_rate as f32;
            // Later windows continue the text after a space
            let separator = if texts.is_empty() { "" } else { " " };
            let tokens = decode_window(
                &mut decoder,
                cached.as_deref_mut(),
                &encoder_val,
                prompt,
                &profile,
                profile.max_decode_steps(window_secs),
                tokenizer,
                separator,
                &mut callback,
            )?;
            let text = tokenizer
                .decode(&tokens, true)
                .map_err(|e| anyhow!(e.to_string()))?
                .trim()
                .to_string();
            if !text.is_empty() {
                texts.push(text);
            }
        }

        tracing::debug!(
            "⏱️ [BENCHMARK STT] Total STT Execution Time: {:?}",
            t_start.elapsed()
        );
        if texts.is_empty() {
            return Err(anyhow!("STT produced no transcript text"));
        }
        Ok(texts.join(" "))
    }

    /// The decoder's starting tokens for one window: start of transcript, then (Whisper) the
    /// language, the task and "no timestamps". The language is the requested one, else the one the
    /// decoder rates most likely for this window among the speaker's languages (or all it knows).
    fn decoder_prompt(
        &self,
        decoder: &mut ort::session::Session,
        encoder_val: &Value,
        profile: &families::SttFamilyProfile,
        options: &crate::media::SttOptions,
    ) -> Result<Vec<i64>> {
        let mut prompt: Vec<i64> = vec![profile.decoder_start_token];
        let (Some(transcribe), Some(translate), Some(no_timestamps)) = (
            profile.transcribe_token,
            profile.translate_token,
            profile.no_timestamps_token,
        ) else {
            if options.translate {
                return Err(anyhow!("Speech translation is not supported by this STT model"));
            }
            return Ok(prompt);
        };
        let tokenizer = self
            .tokenizer
            .as_deref()
            .ok_or_else(|| anyhow!("STT tokenizer missing"))?;
        let vocab = tokenizer.get_vocab(true);
        let clean_lang = options
            .language
            .as_deref()
            .unwrap_or("auto")
            .trim()
            .to_lowercase();

        let mut chosen: Option<i64> = None;
        // 1. Explicit language
        if !clean_lang.is_empty() && clean_lang != "auto" {
            chosen = [
                format!("<|{clean_lang}|>"),
                clean_lang.clone(),
                format!("<|code_{clean_lang}|>"),
            ]
            .iter()
            .find_map(|token| vocab.get(token).map(|&id| id as i64));
            if chosen.is_none() {
                return Err(anyhow!("Requested language is not present in the STT tokenizer"));
            }
        }

        // 2. The speaker's languages this model knows (unknown ones are skipped; none known: all)
        if chosen.is_none() {
            let wanted: Vec<String> = options
                .languages
                .iter()
                .map(|code| families::base_language(code))
                .filter(|code| !code.is_empty())
                .collect();
            let mut all_lang_ids: Vec<i64> = Vec::new();
            let mut lang_token_ids: Vec<i64> = Vec::new();
            for (tok_str, &tok_id) in vocab.iter() {
                if let Some(code) = families::language_of_token(tok_str) {
                    all_lang_ids.push(tok_id as i64);
                    if wanted.iter().any(|w| w == code) {
                        lang_token_ids.push(tok_id as i64);
                    }
                }
            }
            if lang_token_ids.is_empty() {
                if !wanted.is_empty() {
                    tracing::debug!("STT model knows none of the speaker's languages; detecting among all");
                }
                lang_token_ids = all_lang_ids;
            }
            chosen = if lang_token_ids.len() == 1 {
                // One language left: it is the language, no detection needed
                lang_token_ids.first().copied()
            } else {
                detect_language(decoder, encoder_val, profile.decoder_start_token, &lang_token_ids, tokenizer)
            };
        }

        if let Some(lang_id) = chosen {
            prompt.push(lang_id);
        }
        prompt.push(if options.translate { translate } else { transcribe });
        prompt.push(no_timestamps);
        Ok(prompt)
    }
}

/// The language token the decoder rates highest right after "start of transcript" (Whisper's
/// language identification), among `candidates`.
fn detect_language(
    decoder: &mut ort::session::Session,
    encoder_val: &Value,
    start_token: i64,
    candidates: &[i64],
    tokenizer: &tokenizers::Tokenizer,
) -> Option<i64> {
    let ids: Value = Value::from_array(([1usize, 1usize], vec![start_token])).ok()?.into();
    let outputs = decoder.run(decoder_inputs(decoder, &ids, encoder_val)).ok()?;
    let (shape, data) = outputs.get("logits")?.try_extract_tensor::<f32>().ok()?;
    let logits = last_position(&shape.iter().map(|&s| s as usize).collect::<Vec<_>>(), data)?;
    let best = candidates
        .iter()
        .copied()
        .filter(|&id| (id as usize) < logits.len())
        .max_by(|&a, &b| logits[a as usize].total_cmp(&logits[b as usize]))?;
    if let Ok(name) = tokenizer.decode(&[best as u32], false) {
        tracing::debug!("🌍 [STT Dynamic Probing] Auto-Detected Language Token: '{}' (ID: {})", name.trim(), best);
    }
    Some(best)
}

/// Inputs of the uncached decoder: the token ids and the encoder's hidden states.
fn decoder_inputs<'v>(
    decoder: &ort::session::Session,
    ids: &'v Value,
    encoder_val: &'v Value,
) -> HashMap<String, &'v Value> {
    decoder
        .inputs()
        .iter()
        .filter_map(|input| {
            let name = input.name();
            if name.contains("input_ids") {
                Some((name.to_owned(), ids))
            } else if name.contains("encoder_hidden_states") {
                Some((name.to_owned(), encoder_val))
            } else {
                None
            }
        })
        .collect()
}

/// The logits of the last position of a `[batch, sequence, vocab]` (or `[sequence, vocab]`) tensor.
fn last_position<'d>(shape: &[usize], data: &'d [f32]) -> Option<&'d [f32]> {
    let vocab = *shape.last()?;
    let rows = match shape.len() {
        0 | 1 => 1,
        2 => shape[0],
        _ => shape[shape.len() - 2],
    };
    let offset = rows.checked_sub(1)?.checked_mul(vocab)?;
    data.get(offset..offset.checked_add(vocab)?)
}

/// Greedy choice of the next token: the highest score, never the start token, scores of tokens
/// already said lowered (so the decoder does not repeat itself).
fn next_token(
    logits: &[f32],
    start_token: i64,
    said: &std::collections::HashSet<u32>,
) -> Result<u32> {
    const REPETITION_PENALTY: f32 = 1.25;
    let mut best: Option<(u32, f32)> = None;
    for (idx, &val) in logits.iter().enumerate() {
        let tok = idx as u32;
        if tok == start_token as u32 {
            continue;
        }
        let score = if said.contains(&tok) {
            if val < 0.0 { val * REPETITION_PENALTY } else { val / REPETITION_PENALTY }
        } else {
            val
        };
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((tok, score));
        }
    }
    match best {
        Some((tok, score)) if score.is_finite() => Ok(tok),
        _ => Err(anyhow!("STT logits contain no finite selectable score")),
    }
}

/// Decodes one window's transcript tokens (after `prompt`) until end of text.
///
/// With `cached` (a decoder taking `past_key_values.*`), the first step runs the full decoder,
/// which also returns every layer's keys/values (`present.*`); each later step feeds only the new
/// token plus those keys/values, so a step costs the same however long the transcript is. Without
/// it, every step re-reads the whole sequence. New text reaches `callback` as it is decoded.
#[allow(clippy::too_many_arguments)]
fn decode_window(
    decoder: &mut ort::session::Session,
    mut cached: Option<&mut ort::session::Session>,
    encoder_val: &Value,
    prompt: Vec<i64>,
    profile: &families::SttFamilyProfile,
    max_steps: usize,
    tokenizer: &tokenizers::Tokenizer,
    separator: &str,
    callback: &mut Option<&mut (dyn FnMut(String) -> bool + Send)>,
) -> Result<Vec<u32>> {
    let mut ids = prompt;
    let mut speech_tokens: Vec<u32> = Vec::new();
    let mut said: std::collections::HashSet<u32> = std::collections::HashSet::new();
    // Keys/values by the cached decoder's input name (`past_key_values.<layer>.<kind>.<key|value>`)
    let mut past: HashMap<String, Value> = HashMap::new();
    let mut emitted_bytes = 0usize;
    let mut ended = false;

    for step in 0..max_steps {
        let use_cache = cached.is_some() && !past.is_empty();
        tracing::debug!("🎙️ [STT Decoder] Step {}/{} (cached: {})", step, max_steps, use_cache);
        let logits: Vec<f32> = if let (true, Some(cached_decoder)) = (use_cache, cached.as_deref_mut()) {
            let last = *ids.last().ok_or_else(|| anyhow!("STT decoder has no token to continue from"))?;
            let new_id: Value = Value::from_array(([1usize, 1usize], vec![last]))?.into();
            let mut inputs: HashMap<String, &Value> = HashMap::new();
            for input in cached_decoder.inputs() {
                let name = input.name();
                if name.contains("input_ids") {
                    inputs.insert(name.to_owned(), &new_id);
                } else if let Some(value) = past.get(name) {
                    inputs.insert(name.to_owned(), value);
                } else {
                    return Err(anyhow!("STT cached decoder input {name} has no cached value"));
                }
            }
            let outputs = cached_decoder.run(inputs)?;
            let mut logits = None;
            let mut updates = Vec::new();
            for (name, value) in outputs {
                if name == "logits" {
                    let (shape, data) = value.try_extract_tensor::<f32>()?;
                    let shape: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
                    logits = last_position(&shape, data).map(<[f32]>::to_vec);
                } else if let Some(rest) = name.strip_prefix("present.") {
                    updates.push((format!("past_key_values.{rest}"), value));
                }
            }
            past.extend(updates);
            logits.ok_or_else(|| anyhow!("STT cached decoder returned no usable logits"))?
        } else {
            let seq: Value = Value::from_array(([1usize, ids.len()], ids.clone()))?.into();
            let outputs = decoder.run(decoder_inputs(decoder, &seq, encoder_val))?;
            let mut logits = None;
            for (name, value) in outputs {
                if name == "logits" {
                    let (shape, data) = value.try_extract_tensor::<f32>()?;
                    let shape: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
                    logits = last_position(&shape, data).map(<[f32]>::to_vec);
                } else if let Some(rest) = name.strip_prefix("present.") {
                    if cached.is_some() {
                        past.insert(format!("past_key_values.{rest}"), value);
                    }
                }
            }
            logits.ok_or_else(|| anyhow!("STT logits tensor shape is out of bounds"))?
        };

        let next = next_token(&logits, profile.decoder_start_token, &said)?;
        if next == profile.eos_token as u32 {
            ended = true;
            break;
        }
        speech_tokens.push(next);
        said.insert(next);
        ids.push(next as i64);

        if let Some(cb) = callback.as_mut() {
            if let Ok(full_text) = tokenizer.decode(&speech_tokens, false) {
                // Multi-byte guard: wait while the text ends in an incomplete character
                if !full_text.ends_with('\u{FFFD}') && full_text.len() > emitted_bytes {
                    let delta = &full_text[emitted_bytes..];
                    let delta = if emitted_bytes == 0 {
                        format!("{separator}{}", delta.trim_start())
                    } else {
                        delta.to_string()
                    };
                    if !delta.is_empty() && !cb(delta) {
                        return Err(anyhow!("STT output consumer stopped"));
                    }
                    emitted_bytes = full_text.len();
                }
            }
        }
    }
    if !ended {
        return Err(anyhow!("STT decoder token budget exhausted before end of transcript"));
    }
    Ok(speech_tokens)
}

fn extract_audio_path(prompt: &str) -> Option<String> {
    let tag = "[AUDIO_INPUT:";
    let start = prompt.find(tag)?;
    let rest = &prompt[start + tag.len()..];
    let end = rest.find(']')?;
    Some(rest[..end].trim().to_string())
}

fn extract_parameter(prompt: &str, param_name: &str) -> Option<String> {
    let tag = format!("[{}:", param_name);
    let start = prompt.find(&tag)?;
    let rest = &prompt[start + tag.len()..];
    let end = rest.find(']')?;
    Some(rest[..end].trim().to_string())
}
