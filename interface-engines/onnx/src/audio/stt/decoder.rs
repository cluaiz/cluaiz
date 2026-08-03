use super::super::config::AudioConfig;
use super::loader::load_audio_to_pcm;
use super::spectrogram::compute_log_mel_spectrogram;
use crate::engine::OnnxEngine;
use anyhow::{anyhow, Result};
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
        let is_tts = prompt.contains("[TEXT_INPUT]") || (!prompt.contains("[AUDIO_INPUT") && !prompt.contains("encoder_hidden_states"));
        if is_tts {
            let session_arc = self
                .acquire_session()
                .map_err(|e| anyhow!("Failed to acquire audio TTS session: {}", e))?;
            let mut session_guard = session_arc
                .lock()
                .map_err(|_| anyhow!("Failed to lock ONNX audio TTS session mutex"))?;
            super::super::tts::route_tts_inference(self, &mut session_guard, prompt, self.tokenizer.as_deref())
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
        mut callback: Option<&mut (dyn FnMut(String) -> bool + Send)>,
    ) -> Result<String> {
        let session_arc = self
            .acquire_session()
            .map_err(|e| anyhow!("Failed to acquire audio session: {}", e))?;

        let mut session_guard = session_arc
            .lock()
            .map_err(|_| anyhow!("Failed to lock ONNX audio decoder session mutex"))?;

        let decoder_input_names: Vec<String> = session_guard
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();

        let t_start = std::time::Instant::now();

        let audio_path = extract_audio_path(prompt)
            .ok_or_else(|| anyhow!("No [AUDIO_INPUT: ...] tag found in prompt"))?;

        let req_lang = extract_parameter(prompt, "LANGUAGE").unwrap_or_else(|| "auto".to_string());
        let req_translate = extract_parameter(prompt, "TRANSLATE_TO").unwrap_or_default();

        let config = AudioConfig::from_model_dir(&self.model_dir);
        let t_pcm = std::time::Instant::now();
        let pcm_samples = load_audio_to_pcm(&audio_path, &config)?;
        println!("⏱️ [BENCHMARK STT] Step A - PCM Load Time: {:?} (Audio Duration: {:.2}s, Samples: {})", t_pcm.elapsed(), pcm_samples.len() as f32 / config.sample_rate as f32, pcm_samples.len());

        let t_mel = std::time::Instant::now();
        let (mel_flat, actual_frames) = compute_log_mel_spectrogram(&pcm_samples, &config);
        println!("⏱️ [BENCHMARK STT] Step B - Mel Spectrogram Compute Time: {:?} (actual_frames: {}, padded_to: {})", t_mel.elapsed(), actual_frames, config.max_frames);
        let shape_vec = vec![1usize, config.n_mels, config.max_frames];

        let mut real_encoder_hidden_states: Option<(Vec<usize>, Vec<f32>)> = None;

        let t_enc = std::time::Instant::now();
        if let Some(enc_arc) = &self.encoder_session {
            if let Ok(mut enc_guard) = enc_arc.lock() {
                if let Ok(mel_tensor) = Value::from_array((shape_vec.clone(), mel_flat)) {
                    let enc_input_name = enc_guard.inputs().first().map(|i| i.name().to_string()).unwrap_or_else(|| "input_features".to_string());
                    let mut enc_inputs = HashMap::new();
                    enc_inputs.insert(enc_input_name.as_str(), mel_tensor);

                    if let Ok(outputs) = enc_guard.run(enc_inputs) {
                        for (_, v) in outputs.into_iter() {
                            if let Ok((shape, data_tensor)) = v.try_extract_tensor::<f32>() {
                                let shape_usize: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
                                real_encoder_hidden_states = Some((shape_usize, data_tensor.to_vec()));
                                break;
                            }
                        }
                    }
                }
            }
        }
        println!("⏱️ [BENCHMARK STT] Step C - Encoder ONNX Model Inference Time: {:?}", t_enc.elapsed());

        let audio_duration_secs = (pcm_samples.len() as f32) / (config.sample_rate as f32);
        // Multi-Architecture STT Capacity: Query model's max_target_positions if present, or scale dynamically with audio duration (15 tokens/sec)
        let model_max_cap = config.max_target_positions.unwrap_or(448);
        let dynamic_token_budget = ((audio_duration_secs * 15.0) + 64.0).ceil() as usize;
        let max_speech_len = dynamic_token_budget.clamp(64, model_max_cap);

        let mut speech_tokens: Vec<u32> = Vec::new();
        let mut speech_tokens_set: std::collections::HashSet<u32> = std::collections::HashSet::new();

        let mut chosen_lang_token: Option<i64> = None;
        let transcribe = config.transcribe_token;
        let translate = config.translate_token;
        let no_timestamps = config.no_timestamps_token;

        let clean_lang = req_lang.trim().to_lowercase();

        let encoder_val: Value = if let Some((ref hs_shape, ref hs_vec)) = real_encoder_hidden_states {
            Value::from_array((hs_shape.clone(), hs_vec.clone()))
                .map(|v| v.into())
                .map_err(|e| anyhow!("Failed to construct encoder hidden states tensor: {}", e))?
        } else {
            return Err(anyhow!("STT Encoder session failed or missing. Cannot decode audio without real encoder hidden states. Ensure the model package contains a valid encoder ONNX graph."));
        };

        let is_decoder_ids_name: Vec<bool> = decoder_input_names.iter().map(|n| {
            let n_str = n.as_str();
            n_str.contains("input_ids") || n_str.contains("decoder_input_ids")
        }).collect();

        // 🌐 Dynamic Language Vocabulary Discovery (Zero Hardcoding)
        let mut lang_token_ids: Vec<i64> = Vec::new();
        if let Some(tokenizer) = &self.tokenizer {
            let vocab = tokenizer.get_vocab(true);
            for (tok_str, &tok_id) in vocab.iter() {
                if (tok_str.starts_with("<|") && tok_str.ends_with("|>")) || (tok_str.starts_with("<|code_") && tok_str.ends_with("|>")) {
                    let inner = tok_str.trim_start_matches("<|").trim_start_matches("code_").trim_end_matches("|>");
                    if inner.len() >= 2 && inner.len() <= 5 && inner.chars().all(|c| c.is_ascii_lowercase()) {
                        if !inner.contains("transcribe") && !inner.contains("translate") && !inner.contains("notimestamps") && !inner.contains("startof") {
                            lang_token_ids.push(tok_id as i64);
                        }
                    }
                }
            }
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
        }

        // 2. Auto-Detect Mode: Dynamic Step-0 Logits Probing across ONNX decoder
        if chosen_lang_token.is_none() && !lang_token_ids.is_empty() {
            let probe_ids = vec![config.start_of_transcript];
            if let Ok(input_ids_val) = Value::from_array(([1usize, 1usize], probe_ids)) {
                let input_ids_dyn: Value = input_ids_val.into();
                let mut probe_inputs = HashMap::with_capacity(decoder_input_names.len());
                for (idx, name) in decoder_input_names.iter().enumerate() {
                    if is_decoder_ids_name[idx] {
                        probe_inputs.insert(name.as_str(), &input_ids_dyn);
                    } else {
                        probe_inputs.insert(name.as_str(), &encoder_val);
                    }
                }

                if let Ok(output_tensors) = session_guard.run(probe_inputs) {
                    let mut probe_data: Option<(Vec<usize>, Vec<f32>)> = None;
                    if let Some(logits) = output_tensors.get("logits") {
                        if let Ok((shape, data)) = logits.try_extract_tensor::<f32>() {
                            probe_data = Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
                        }
                    }
                    if probe_data.is_none() {
                        for (_, v) in output_tensors.iter() {
                            if let Ok((shape, data)) = v.try_extract_tensor::<f32>() {
                                probe_data = Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
                                break;
                            }
                        }
                    }

                    if let Some((shape, data)) = probe_data {
                        let vocab_size = *shape.last().unwrap_or(&51866);
                        let offset = (shape.get(1).cloned().unwrap_or(1).saturating_sub(1)) * vocab_size;
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
                                    if let Ok(lang_name) = tokenizer.decode(&[detected_id as u32], false) {
                                        println!("🌍 [Whisper STT Dynamic Probing] Auto-Detected Language Token: '{}' (ID: {}, Score: {:.4})", lang_name.trim(), detected_id, max_score);
                                    }
                                }
                                chosen_lang_token = Some(detected_id);
                            }
                        }
                    }
                }
            }
        }

        let task_token = if !req_translate.is_empty() && req_translate.to_lowercase() != "false" {
            translate
        } else {
            transcribe
        };

        let mut current_decoder_ids: Vec<i64> = Vec::new();
        current_decoder_ids.push(config.start_of_transcript);

        if let Some(lang_id) = chosen_lang_token {
            current_decoder_ids.push(lang_id);
        }
        current_decoder_ids.push(task_token);
        current_decoder_ids.push(no_timestamps);

        println!("🔍 [Whisper Decoder Inputs]: {:?}", decoder_input_names);

        let mut emitted_bytes_offset = 0usize;

        for step in 0..max_speech_len {
            let seq_len = current_decoder_ids.len();
            let input_ids_val = match Value::from_array(([1usize, seq_len], current_decoder_ids.clone())) {
                Ok(v) => v,
                Err(e) => {
                    println!("❌ [Whisper STT Decoder] Value::from_array failed on step {}: {}", step, e);
                    break;
                }
            };
            let input_ids_dyn: Value = input_ids_val.into();

            let mut step_inputs: HashMap<&str, &Value> = HashMap::with_capacity(decoder_input_names.len());

            for (idx, name) in decoder_input_names.iter().enumerate() {
                if is_decoder_ids_name[idx] {
                    step_inputs.insert(name.as_str(), &input_ids_dyn);
                } else {
                    step_inputs.insert(name.as_str(), &encoder_val);
                }
            }

            println!("🎙️ [Whisper STT Decoder] Step {}/{} running session with {} inputs...", step, max_speech_len, step_inputs.len());

            let output_tensors = match session_guard.run(step_inputs) {
                Ok(out) => out,
                Err(e) => {
                    println!("❌ [ORT Decoder Run Error]: {}", e);
                    eprintln!("ORT decoder run error: {}", e);
                    break;
                }
            };

            let mut raw_extracted: Option<(Vec<usize>, Vec<f32>)> = None;

            if let Some(logits) = output_tensors.get("logits") {
                if let Ok((shape, data)) = logits.try_extract_tensor::<f32>() {
                    raw_extracted = Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
                }
            }

            if raw_extracted.is_none() {
                for (k, v) in output_tensors.iter() {
                    println!("🔍 [Whisper STT Decoder] ONNX Output key: '{}'", k);
                    if let Ok((shape, data)) = v.try_extract_tensor::<f32>() {
                        raw_extracted = Some((shape.iter().map(|&s| s as usize).collect(), data.to_vec()));
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
                    println!("⚠️ [Whisper STT Decoder] Logits offset out of bounds: shape={:?}, seq_len={}, vocab_size={}, data.len()={}", shape, seq_len, vocab_size, data.len());
                    break;
                }
                let step_logits = &data[offset..offset + vocab_size];

                let mut best_idx = 0usize;
                let mut max_val = f32::NEG_INFINITY;

                let max_allowed_tok = (step_logits.len().saturating_sub(1)) as u32;
                let rep_penalty = 1.25f32;

                for (idx, &val) in step_logits.iter().enumerate() {
                    let tok_id = idx as u32;
                    if tok_id <= max_allowed_tok && tok_id != config.start_of_transcript as u32 {
                        if speech_tokens.len() < 5 && (tok_id == config.end_of_text_token as u32 || tok_id == 50257 || tok_id == 50256) {
                            continue;
                        }

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
                best_idx as u32
            } else {
                break;
            };

            let is_eot = (next_tok == config.end_of_text_token as u32
                || next_tok == 50257
                || next_tok == 50256) && speech_tokens.len() >= 3;

            if is_eot {
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
                        if !full_text.ends_with('\u{FFFD}') && full_text.len() > emitted_bytes_offset {
                            let delta = &full_text[emitted_bytes_offset..];
                            if !delta.is_empty() {
                                cb(delta.to_string());
                            }
                            emitted_bytes_offset = full_text.len();
                        }
                    }
                }
            }
        }

        println!("⏱️ [BENCHMARK STT] Total STT Execution Time: {:?}", t_start.elapsed());

        if speech_tokens.is_empty() {
            return Err(anyhow!("STT Decoder failed to produce tokens."));
        }

        if let Some(tokenizer) = &self.tokenizer {
            let decoded = tokenizer.decode(&speech_tokens, true).unwrap_or_default();
            let cleaned = decoded.trim().to_string();
            if cleaned.is_empty() {
                Ok(".".to_string())
            } else {
                Ok(cleaned)
            }
        } else {
            Ok(format!("[Audio Decoded Tokens: {:?}]", speech_tokens))
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
