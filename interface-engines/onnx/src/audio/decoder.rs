use super::config::AudioConfig;
use super::loader::load_audio_to_pcm;
use super::spectrogram::compute_log_mel_spectrogram;
use crate::engine::OnnxEngine;
use anyhow::{anyhow, Result};
use ort::value::Value;
use std::collections::HashMap;

impl OnnxEngine {
    pub fn execute_audio_graph(&self, prompt: &str) -> Result<String> {
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

        // ─── 1. Determine Model Modality & Task ─────────────────────────────
        let is_tts = decoder_input_names.iter().any(|n: &String| {
            n.contains("text")
                || n.contains("input_ids")
                || n.contains("phonemes")
                || n.contains("style")
        }) && !decoder_input_names
            .iter()
            .any(|n: &String| n.contains("encoder_hidden_states"));

        let is_audio_embedding = decoder_input_names
            .iter()
            .any(|n: &String| n.contains("waveform") || n.contains("audio_embed"));

        // ─── 2. TTS (Text-to-Speech) Execution Graph ───────────────────────
        if is_tts {
            let text_input = extract_parameter(prompt, "TEXT_INPUT")
                .or_else(|| extract_clean_text_prompt(prompt))
                .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?;

            let mut tts_inputs: HashMap<String, Value> = HashMap::new();

            for name in &decoder_input_names {
                let name_str: &str = name.as_str();
                if name_str.contains("input_ids")
                    || name_str.contains("tokens")
                    || name_str.contains("text")
                {
                    let token_ids: Vec<i64> = text_input.bytes().map(|b| b as i64).collect();
                    let seq_len = token_ids.len();
                    if let Ok(val) = Value::from_array(([1usize, seq_len], token_ids)) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                } else if name_str.contains("style") || name_str.contains("voice") {
                    let dummy_style = vec![0.0f32; 128];
                    if let Ok(val) = Value::from_array(([1usize, 128], dummy_style)) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                } else if name_str.contains("speed") {
                    if let Ok(val) = Value::from_array(([1usize], vec![1.0f32])) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                }
            }

            let output_tensors = session_guard
                .run(tts_inputs)
                .map_err(|e| anyhow!("TTS ONNX graph execution failed: {}", e))?;

            if let Some(val) = output_tensors.values().next() {
                if let Ok((_shape, wav_tensor)) = val.try_extract_tensor::<f32>() {
                    use base64::Engine;
                    let raw_bytes: Vec<u8> = wav_tensor
                        .iter()
                        .flat_map(|f: &f32| f.to_le_bytes())
                        .collect();
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);
                    return Ok(format!("data:audio/wav;base64,{}", b64));
                }
            }
            return Err(anyhow!("TTS graph produced empty waveform output."));
        }

        // ─── 3. STT (Speech-to-Text) Execution Graph ───────────────────────
        let audio_path = extract_audio_path(prompt)
            .ok_or_else(|| anyhow!("No [AUDIO_INPUT: ...] tag found in prompt"))?;

        let req_lang = extract_parameter(prompt, "LANGUAGE").unwrap_or_else(|| "auto".to_string());
        let req_translate = extract_parameter(prompt, "TRANSLATE_TO").unwrap_or_default();

        let config = AudioConfig::from_model_dir(&self.model_dir);
        let pcm_samples = load_audio_to_pcm(&audio_path, &config)?;

        if is_audio_embedding {
            let sample_len = pcm_samples.len();
            let mut emb_inputs: HashMap<String, Value> = HashMap::new();
            if let Ok(val) = Value::from_array(([1usize, sample_len], pcm_samples.clone())) {
                emb_inputs.insert(decoder_input_names[0].clone(), val.into());
                let output = session_guard.run(emb_inputs)?;
                if let Some(val) = output.values().next() {
                    if let Ok((_shape, data_tensor)) = val.try_extract_tensor::<f32>() {
                        let slice = data_tensor;
                        return Ok(format!("{:?}", &slice[..slice.len().min(16)]));
                    }
                }
            }
        }

        let mel_flat = compute_log_mel_spectrogram(&pcm_samples, &config);
        let shape_vec = vec![1usize, config.n_mels, config.max_frames];

        let mut real_encoder_hidden_states: Option<(Vec<usize>, Vec<f32>)> = None;

        if let Some(enc_arc) = &self.encoder_session {
            if let Ok(mut enc_guard) = enc_arc.lock() {
                if let Ok(mel_tensor) = Value::from_array((shape_vec.clone(), mel_flat)) {
                    let mut enc_inputs = HashMap::new();
                    enc_inputs.insert("input_features", mel_tensor);

                    if let Ok(outputs) = enc_guard.run(enc_inputs) {
                        for (_, v) in outputs.into_iter() {
                            if let Ok((shape, data_tensor)) = v.try_extract_tensor::<f32>() {
                                let shape_usize: Vec<usize> =
                                    shape.iter().map(|&d| d as usize).collect();
                                let data_vec: Vec<f32> = data_tensor.to_vec();
                                real_encoder_hidden_states = Some((shape_usize, data_vec));
                            }
                        }
                    }
                }
            }
        }

        let max_speech_len = 224;
        let mut speech_tokens: Vec<u32> = Vec::new();

        let mut chosen_lang_token: Option<i64> = None;
        let transcribe = config.transcribe_token;
        let translate = config.translate_token;
        let no_timestamps = config.no_timestamps_token;

        let clean_lang = req_lang.trim().to_lowercase();

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

        let task_token = if !req_translate.is_empty() && req_translate.to_lowercase() != "false" {
            translate
        } else {
            transcribe
        };

        let mut current_decoder_ids: Vec<i64> = Vec::new();
        current_decoder_ids.push(config.start_of_transcript);

        if let Some(lang_id) = chosen_lang_token {
            current_decoder_ids.push(lang_id);
            current_decoder_ids.push(task_token);
            current_decoder_ids.push(no_timestamps);
        }

        let mut is_auto_detecting = chosen_lang_token.is_none();

        for _step in 0..max_speech_len {
            let mut step_inputs: HashMap<String, Value> = HashMap::new();

            for name in &decoder_input_names {
                let name_str: &str = name.as_str();
                if name_str.contains("input_ids") || name_str.contains("decoder_input_ids") {
                    let seq_len = current_decoder_ids.len();
                    if let Ok(val) =
                        Value::from_array(([1usize, seq_len], current_decoder_ids.clone()))
                    {
                        step_inputs.insert(name.clone(), val.into());
                    }
                } else {
                    if let Some((ref hs_shape, ref hs_data)) = real_encoder_hidden_states {
                        if let Ok(val) = Value::from_array((hs_shape.clone(), hs_data.clone())) {
                            step_inputs.insert(name.clone(), val.into());
                            continue;
                        }
                    }
                    let dummy_shape = vec![1usize, 1500, 1280];
                    let dummy_data = vec![0.0f32; 1 * 1500 * 1280];
                    if let Ok(val) = Value::from_array((dummy_shape, dummy_data)) {
                        step_inputs.insert(name.clone(), val.into());
                    }
                }
            }

            let mut last_ort_err = String::new();
            let output_tensors = match session_guard.run(step_inputs) {
                Ok(out) => out,
                Err(e) => {
                    last_ort_err = format!("ORT decoder run error: {}", e);
                    eprintln!("{}", last_ort_err);
                    break;
                }
            };

            let logits_val = output_tensors
                .iter()
                .find(|(k, _)| k.contains("logits"))
                .map(|(_, v)| v)
                .or_else(|| output_tensors.values().next());

            let next_tok = if let Some(logits) = logits_val {
                let (shape, data_tensor) = match logits.try_extract_tensor::<f32>() {
                    Ok(res) => res,
                    Err(_) => break,
                };
                let data = data_tensor;
                let seq_len = shape.get(1).cloned().unwrap_or(1) as usize;
                let vocab_size = shape.get(2).cloned().unwrap_or(51866) as usize;
                let offset = (seq_len.saturating_sub(1)) * vocab_size;

                if offset + vocab_size > data.len() {
                    break;
                }
                let step_logits = &data[offset..offset + vocab_size];

                let mut best_idx = 0usize;
                let mut max_val = f32::NEG_INFINITY;

                if is_auto_detecting {
                    for idx in 50259..=50358 {
                        if idx < step_logits.len() {
                            let val = step_logits[idx];
                            if val > max_val {
                                max_val = val;
                                best_idx = idx;
                            }
                        }
                    }
                } else {
                    let max_allowed_tok = config.end_of_text_token as u32;
                    let rep_penalty = 1.25f32;

                    for (idx, &val) in step_logits.iter().enumerate() {
                        let tok_id = idx as u32;
                        if (tok_id <= max_allowed_tok)
                            && tok_id != config.start_of_transcript as u32
                        {
                            if speech_tokens.len() < 2 && tok_id == config.end_of_text_token as u32
                            {
                                continue;
                            }

                            let mut adjusted_val = val;
                            // Apply repetition penalty to already predicted tokens
                            if speech_tokens.contains(&tok_id) {
                                adjusted_val = if adjusted_val < 0.0 {
                                    adjusted_val * rep_penalty
                                } else {
                                    adjusted_val / rep_penalty
                                };
                            }

                            // Prevent 3-gram identical repeat loop
                            let len = speech_tokens.len();
                            if len >= 3
                                && tok_id == speech_tokens[len - 1]
                                && tok_id == speech_tokens[len - 2]
                            {
                                continue;
                            }

                            if adjusted_val > max_val {
                                max_val = adjusted_val;
                                best_idx = idx;
                            }
                        }
                    }
                }
                best_idx as u32
            } else {
                break;
            };

            if is_auto_detecting {
                is_auto_detecting = false;
                current_decoder_ids.push(next_tok as i64);
                current_decoder_ids.push(task_token);
                current_decoder_ids.push(no_timestamps);
                continue;
            }

            if next_tok == config.end_of_text_token as u32 {
                break;
            }

            speech_tokens.push(next_tok);
            current_decoder_ids.push(next_tok as i64);
        }

        if speech_tokens.is_empty() {
            return Err(anyhow!("ONNX/FFI Audio Kernel failed to produce output tokens. Decoder step run failed."));
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

fn extract_clean_text_prompt(prompt: &str) -> Option<String> {
    if prompt.starts_with("[TEXT_INPUT]") {
        Some(prompt.trim_start_matches("[TEXT_INPUT]").trim().to_string())
    } else {
        Some(prompt.trim().to_string())
    }
}
