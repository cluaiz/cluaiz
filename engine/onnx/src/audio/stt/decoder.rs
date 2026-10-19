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

        let mut session_guard = session_arc
            .lock()
            .map_err(|_| anyhow!("Failed to lock ONNX audio decoder session mutex"))?;
        if !crate::media::is_uncached_decoder(&session_guard) {
            return Err(anyhow!("STT requires a validated uncached decoder; unsupported cache inputs are not synthesized"));
        }

        let decoder_input_names: Vec<String> = session_guard
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();

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
        if pcm_samples.len() > profile.max_samples {
            return Err(anyhow!(
                "Audio exceeds this STT processor's supported window; split it into chunks"
            ));
        }
        tracing::debug!(
            "⏱️ [BENCHMARK STT] Step A - PCM Load Time: {:?} (Audio Duration: {:.2}s, Samples: {})",
            t_pcm.elapsed(),
            pcm_samples.len() as f32 / profile.sample_rate as f32,
            pcm_samples.len()
        );

        let t_enc = std::time::Instant::now();
        let real_encoder_hidden_states = match profile.encoder_kind {
            SttEncoderKind::MelSpectrogram { .. } => {
                families::whisper::encode_whisper_features(
                    &mut encoder,
                    pcm_samples,
                    &profile,
                    limits,
                )?
            }
            SttEncoderKind::RawWaveform { .. } => {
                families::moonshine::encode_moonshine_waveform(
                    &mut encoder,
                    pcm_samples,
                    &profile,
                    limits,
                )?
            }
        };
        drop(encoder);
        tracing::debug!(
            "⏱️ [BENCHMARK STT] Step C - Encoder ONNX Model Inference Time: {:?}",
            t_enc.elapsed()
        );

        let audio_duration_secs = (pcm_samples.len() as f32) / (profile.sample_rate as f32);
        let max_speech_len = profile.max_decode_steps(audio_duration_secs);

        let mut speech_tokens: Vec<u32> = Vec::new();
        let mut speech_tokens_set: std::collections::HashSet<u32> =
            std::collections::HashSet::new();

        let clean_lang = req_lang.trim().to_lowercase();

        let encoder_val: Value = {
            Value::from_array(real_encoder_hidden_states)
                .map(|v| v.into())
                .map_err(|e| anyhow!("Failed to construct encoder hidden states tensor: {}", e))?
        };

        let is_decoder_ids_name: Vec<bool> = decoder_input_names
            .iter()
            .map(|n| {
                let n_str = n.as_str();
                n_str.contains("input_ids") || n_str.contains("decoder_input_ids")
            })
            .collect();

        let mut current_decoder_ids: Vec<i64> = vec![profile.decoder_start_token];

        if let (Some(transcribe), Some(translate), Some(no_timestamps)) = (
            profile.transcribe_token,
            profile.translate_token,
            profile.no_timestamps_token,
        ) {
            let mut chosen_lang_token: Option<i64> = None;

            // 🌐 Dynamic Language Vocabulary Discovery (Zero Hardcoding), narrowed to the speaker's
            // languages when given: a phrase is then detected among only those
            let wanted: Vec<String> = options
                .languages
                .iter()
                .map(|code| families::base_language(code))
                .filter(|code| !code.is_empty())
                .collect();
            let mut lang_token_ids: Vec<i64> = Vec::new();
            if let Some(tokenizer) = &self.tokenizer {
                let vocab = tokenizer.get_vocab(true);
                for (tok_str, &tok_id) in vocab.iter() {
                    if let Some(code) = families::language_of_token(tok_str) {
                        if wanted.is_empty() || wanted.iter().any(|w| w == code) {
                            lang_token_ids.push(tok_id as i64);
                        }
                    }
                }
            }
            if !wanted.is_empty() && lang_token_ids.is_empty() {
                return Err(anyhow!(
                    "None of the speaker's languages is known to this STT model"
                ));
            }
            // One language left: it is the language, no detection needed
            if lang_token_ids.len() == 1 && (clean_lang.is_empty() || clean_lang == "auto") {
                chosen_lang_token = lang_token_ids.first().copied();
            }

            // 1. Explicit Language Mode Resolution
            if !clean_lang.is_empty() && clean_lang != "auto" {
                if let Some(tokenizer) = &self.tokenizer {
                    let vocab = tokenizer.get_vocab(true);
                    let target1 = format!("<|{}|>", clean_lang);
                    let target2 = clean_lang.clone();
                    let target3 = format!("<|code_{}|>", clean_lang);

                    if let Some(&id) = vocab.get(&target1) {
                        chosen_lang_token = Some(id as i64);
                    } else if let Some(&id) = vocab.get(&target2) {
                        chosen_lang_token = Some(id as i64);
                    } else if let Some(&id) = vocab.get(&target3) {
                        chosen_lang_token = Some(id as i64);
                    }
                }
                if chosen_lang_token.is_none() {
                    return Err(anyhow!(
                        "Requested language is not present in the STT tokenizer"
                    ));
                }
            }

            // 2. Auto-Detect Mode: Dynamic Step-0 Logits Probing across ONNX decoder
            if chosen_lang_token.is_none() && !lang_token_ids.is_empty() {
                let probe_ids = vec![profile.decoder_start_token];
                if let Ok(input_ids_val) = Value::from_array(([1usize, 1usize], probe_ids)) {
                    let input_ids_dyn: Value = input_ids_val.into();

                    let mut probe_inputs = HashMap::with_capacity(decoder_input_names.len());
                    for (idx, name) in decoder_input_names.iter().enumerate() {
                        let n_str = name.as_str();
                        if is_decoder_ids_name[idx] {
                            probe_inputs.insert(n_str, &input_ids_dyn);
                        } else if n_str.contains("encoder_hidden_states") {
                            probe_inputs.insert(n_str, &encoder_val);
                        }
                    }

                    if let Ok(output_tensors) = session_guard.run(probe_inputs) {
                        let mut probe_data: Option<(Vec<usize>, Vec<f32>)> = None;
                        if let Some(logits) = output_tensors.get("logits") {
                            if let Ok((shape, data)) = logits.try_extract_tensor::<f32>() {
                                probe_data = Some((
                                    shape.iter().map(|&s| s as usize).collect(),
                                    data.to_vec(),
                                ));
                            }
                        }
                        if probe_data.is_none() {
                            for (_, v) in output_tensors.iter() {
                                if let Ok((shape, data)) = v.try_extract_tensor::<f32>() {
                                    probe_data = Some((
                                        shape.iter().map(|&s| s as usize).collect(),
                                        data.to_vec(),
                                    ));
                                    break;
                                }
                            }
                        }

                        if let Some((shape, data)) = probe_data {
                            let vocab_size = *shape.last().unwrap_or(&51866);
                            let offset =
                                (shape.get(1).cloned().unwrap_or(1).saturating_sub(1)) * vocab_size;
                            if offset + vocab_size <= data.len() {
                                let logits_slice = &data[offset..offset + vocab_size];
                                let mut max_score = f32::NEG_INFINITY;
                                let mut best_lang_id = None;

                                for &tid in &lang_token_ids {
                                    let idx = tid as usize;
                                    if idx < logits_slice.len() {
                                        let score = logits_slice[idx];
                                        if score > max_score {
                                            max_score = score;
                                            best_lang_id = Some(tid);
                                        }
                                    }
                                }

                                if let Some(detected_id) = best_lang_id {
                                    if let Some(tokenizer) = &self.tokenizer {
                                        if let Ok(lang_name) =
                                            tokenizer.decode(&[detected_id as u32], false)
                                        {
                                            tracing::debug!("🌍 [STT Dynamic Probing] Auto-Detected Language Token: '{}' (ID: {}, Score: {:.4})", lang_name.trim(), detected_id, max_score);
                                        }
                                    }
                                    chosen_lang_token = Some(detected_id);
                                }
                            }
                        }
                    }
                }
            }

            let task_token = if options.translate {
                translate
            } else {
                transcribe
            };
            if let Some(lang_id) = chosen_lang_token {
                current_decoder_ids.push(lang_id);
            }
            current_decoder_ids.push(task_token);
            current_decoder_ids.push(no_timestamps);
        } else {
            if options.translate {
                return Err(anyhow!(
                    "Speech translation is not supported by this STT model"
                ));
            }
        }

        tracing::debug!("🔍 [STT Decoder Inputs]: {:?}", decoder_input_names);

        let mut emitted_bytes_offset = 0usize;
        let mut ended = false;

        for step in 0..max_speech_len {
            let seq_len = current_decoder_ids.len();
            let input_ids_val =
                Value::from_array(([1usize, seq_len], current_decoder_ids.clone()))?;
            let input_ids_dyn: Value = input_ids_val.into();

            let mut step_inputs: HashMap<&str, &Value> =
                HashMap::with_capacity(decoder_input_names.len());

            for (idx, name) in decoder_input_names.iter().enumerate() {
                let n_str = name.as_str();
                if is_decoder_ids_name[idx] {
                    step_inputs.insert(n_str, &input_ids_dyn);
                } else if n_str.contains("encoder_hidden_states") {
                    step_inputs.insert(n_str, &encoder_val);
                }
            }

            tracing::debug!(
                "🎙️ [STT Decoder] Step {}/{} running session with {} inputs...",
                step,
                max_speech_len,
                step_inputs.len()
            );

            let output_tensors = session_guard.run(step_inputs)?;

            let mut raw_extracted: Option<(Vec<usize>, Vec<f32>)> = None;

            if let Some(logits) = output_tensors.get("logits") {
                if let Ok((shape, data)) = logits.try_extract_tensor::<f32>() {
                    raw_extracted =
                        Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
                }
            }

            if raw_extracted.is_none() {
                for (k, v) in output_tensors.iter() {
                    tracing::debug!("🔍 [STT Decoder] ONNX Output key: '{}'", k);
                    if let Ok((shape, data)) = v.try_extract_tensor::<f32>() {
                        raw_extracted =
                            Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
                        break;
                    }
                }
            }

            let next_tok = if let Some((shape, data)) = raw_extracted {
                let (seq_len, vocab_size) = if shape.len() >= 3 {
                    (shape[1], shape[2])
                } else if shape.len() == 2 {
                    (shape[0], shape[1])
                } else {
                    (1, shape.last().cloned().unwrap_or(51866))
                };
                let offset = (seq_len.saturating_sub(1)) * vocab_size;

                if offset + vocab_size > data.len() {
                    tracing::debug!("⚠️ [STT Decoder] Logits offset out of bounds: shape={:?}, seq_len={}, vocab_size={}, data.len()={}", shape, seq_len, vocab_size, data.len());
                    return Err(anyhow!("STT logits tensor shape is out of bounds"));
                }
                let step_logits = &data[offset..offset + vocab_size];

                let mut best_idx = 0usize;
                let mut max_val = f32::NEG_INFINITY;

                let max_allowed_tok = (step_logits.len().saturating_sub(1)) as u32;
                let rep_penalty = 1.25f32;

                for (idx, &val) in step_logits.iter().enumerate() {
                    let tok_id = idx as u32;
                    if tok_id <= max_allowed_tok && tok_id != profile.decoder_start_token as u32 {
                        let mut adjusted_val = val;
                        if speech_tokens_set.contains(&tok_id) {
                            adjusted_val = if adjusted_val < 0.0 {
                                adjusted_val * rep_penalty
                            } else {
                                adjusted_val / rep_penalty
                            };
                        }

                        if adjusted_val > max_val {
                            max_val = adjusted_val;
                            best_idx = idx;
                        }
                    }
                }
                if !max_val.is_finite() {
                    return Err(anyhow!("STT logits contain no finite selectable score"));
                }
                best_idx as u32
            } else {
                return Err(anyhow!("STT decoder returned no usable logits"));
            };

            let is_eot = next_tok == profile.eos_token as u32;

            if is_eot {
                ended = true;
                break;
            }

            speech_tokens.push(next_tok);
            speech_tokens_set.insert(next_tok);
            current_decoder_ids.push(next_tok as i64);

            if let Some(cb) = callback.as_mut() {
                if let Some(tokenizer) = &self.tokenizer {
                    if let Ok(full_text) = tokenizer.decode(&speech_tokens, false) {
                        // Universal Multi-Byte UTF-8 Boundary Guard:
                        // Defer emission if the token sequence ends in incomplete multi-byte bytes ('\u{FFFD}')
                        if !full_text.ends_with('\u{FFFD}')
                            && full_text.len() > emitted_bytes_offset
                        {
                            let delta = &full_text[emitted_bytes_offset..];
                            if !delta.is_empty() && !cb(delta.to_string()) {
                                return Err(anyhow!("STT output consumer stopped"));
                            }
                            emitted_bytes_offset = full_text.len();
                        }
                    }
                }
            }
        }

        tracing::debug!(
            "⏱️ [BENCHMARK STT] Total STT Execution Time: {:?}",
            t_start.elapsed()
        );

        if !ended {
            return Err(anyhow!(
                "STT decoder token budget exhausted before end of transcript"
            ));
        }
        if speech_tokens.is_empty() {
            return Err(anyhow!("STT Decoder failed to produce tokens."));
        }

        if let Some(tokenizer) = &self.tokenizer {
            let decoded = tokenizer
                .decode(&speech_tokens, true)
                .map_err(|e| anyhow!(e.to_string()))?;
            let cleaned = decoded.trim().to_string();
            if cleaned.is_empty() {
                Err(anyhow!("STT produced no transcript text"))
            } else {
                Ok(cleaned)
            }
        } else {
            Err(anyhow!("STT tokenizer missing"))
        }
    }
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
