use super::config::AudioConfig;
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
        let decoder_output_names: Vec<String> = session_guard
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        eprintln!("🔊 [ONNX Audio Probe] Decoder Inputs: {:?}", decoder_input_names);
        eprintln!("🔊 [ONNX Audio Probe] Decoder Outputs: {:?}", decoder_output_names);

        // ─── 1. Determine Model Modality & Task ─────────────────────────────
        let is_asr = decoder_input_names
            .iter()
            .any(|n: &String| n.contains("encoder_hidden_states") || n.contains("audio_features") || n.contains("mel"));

        let is_tts = !is_asr && decoder_input_names.iter().any(|n: &String| {
            n.contains("text")
                || n.contains("input_ids")
                || n.contains("phonemes")
                || n.contains("speech")
                || n.contains("feat")
                || n.contains("input")
        });

        let is_audio_embedding = decoder_input_names
            .iter()
            .any(|n: &String| n.contains("waveform") || n.contains("audio_embed"));

        eprintln!("🔊 [ONNX Audio Probe] Detected Flags -> is_tts: {}, is_asr: {}, is_audio_embedding: {}", is_tts, is_asr, is_audio_embedding);

        // ─── 2. TTS (Text-to-Speech) Execution Graph ───────────────────────
        if is_tts {
            let text_input = extract_parameter(prompt, "TEXT_INPUT")
                .or_else(|| extract_clean_text_prompt(prompt))
                .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?;

            eprintln!("🔊 [ONNX Audio Probe] Processing TTS Text Input: '{}'", text_input);

            // Check if model is Flow Estimator (CosyVoice flow matching graph)
            let is_flow_estimator = decoder_input_names.iter().any(|n| n == "cond" || n == "spks" || n == "mu");

            if is_flow_estimator {
                eprintln!("🔊 [ONNX Native Pipeline] Flow Estimator Graph Detected. Executing Flow-Matching Sampler...");
                let seq_len = (text_input.bytes().count() * 4).max(30);
                let mel_data = super::flow_matching::FlowMatchingSampler::sample_mel_features(&mut session_guard, seq_len, 10)?;
                
                eprintln!("🔊 [ONNX Native Pipeline] Flow Mel-Spectrogram Generated. Synthesizing PCM WAV via Native Vocoder...");
                let vocoder = super::vocoder::NativeVocoder::default();
                let pcm_samples = vocoder.synthesize_mel_to_pcm(&mel_data, seq_len);
                let wav_bytes = vocoder.encode_wav_bytes(&pcm_samples);

                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                return Ok(format!("data:audio/wav;base64,{}", b64));
            }

            let mut tts_inputs: HashMap<String, Value> = HashMap::new();

            for name in &decoder_input_names {
                let name_str: &str = name.as_str();

                if name_str.contains("speed") || name_str.contains("scale") || name_str.contains("pitch") {
                    if let Ok(val) = Value::from_array(([1usize], vec![1.0f32])) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                } else if name_str.contains("style") {
                    let style_vec = vec![0.0f32; 256];
                    if let Ok(val) = Value::from_array(([1usize, 256usize], style_vec)) {
                        tts_inputs.insert(name.clone(), val.into());
                    } else if let Ok(val) = Value::from_array(([1usize], vec![0.0f32])) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                } else if name_str.contains("_len") || name_str.contains("length") {
                    let seq_len = text_input.bytes().count();
                    if let Ok(val) = Value::from_array(([1usize], vec![seq_len as i64])) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                } else if name_str == "input" || name_str.contains("speech") || name_str.contains("audio") || name_str.contains("feat") {
                    let token_ids_f32: Vec<f32> = text_input.bytes().map(|b| b as f32).collect();
                    let seq_len = token_ids_f32.len();
                    
                    // Construct both 80-bin layouts: [1, seq_len, 80] and [1, 80, seq_len]
                    let mut mel_matrix_1 = vec![0.0f32; 1 * seq_len * 80];
                    for i in 0..seq_len {
                        mel_matrix_1[i * 80] = token_ids_f32[i];
                    }

                    if let Ok(val) = Value::from_array(([1usize, seq_len, 80usize], mel_matrix_1)) {
                        tts_inputs.insert(name.clone(), val.into());
                    } else {
                        let mut mel_matrix_2 = vec![0.0f32; 1 * 80 * seq_len];
                        for row in 0..80 {
                            for col in 0..seq_len {
                                mel_matrix_2[row * seq_len + col] = token_ids_f32[col];
                            }
                        }
                        if let Ok(val) = Value::from_array(([1usize, 80usize, seq_len], mel_matrix_2)) {
                            tts_inputs.insert(name.clone(), val.into());
                        } else if let Ok(val) = Value::from_array(([1usize, seq_len], token_ids_f32)) {
                            tts_inputs.insert(name.clone(), val.into());
                        }
                    }
                } else {
                    let token_ids_i64: Vec<i64> = text_input.bytes().map(|b| b as i64).collect();
                    let seq_len = token_ids_i64.len();
                    if let Ok(val) = Value::from_array(([1usize, seq_len], token_ids_i64.clone())) {
                        tts_inputs.insert(name.clone(), val.into());
                    } else if let Ok(val) = Value::from_array(([1usize, 1usize, seq_len], token_ids_i64)) {
                        tts_inputs.insert(name.clone(), val.into());
                    }
                }
            }

            eprintln!("🔊 [ONNX Audio Probe] Input Tensor Map Keys: {:?}", tts_inputs.keys().collect::<Vec<_>>());

            let output_tensors = session_guard
                .run(tts_inputs)
                .map_err(|e| anyhow!("TTS ONNX graph execution failed: {}", e))?;

            eprintln!("🔊 [ONNX Audio Probe] Output Tensor Map Keys: {:?}", output_tensors.keys().collect::<Vec<_>>());

            if let Some(val) = output_tensors.values().next() {
                if let Ok((shape, wav_tensor)) = val.try_extract_tensor::<f32>() {
                    eprintln!("🔊 [ONNX Audio Probe] Extracted f32 audio tensor shape: {:?}, samples count: {}", shape, wav_tensor.len());
                    if wav_tensor.is_empty() {
                        return Err(anyhow!("ONNX model output f32 tensor was empty (0 samples)."));
                    }

                    let vocoder = super::vocoder::NativeVocoder::default();
                    let pcm_samples = if shape.len() >= 2 && shape[1] == 80 {
                        vocoder.synthesize_mel_to_pcm(&wav_tensor, shape[shape.len() - 1] as usize)
                    } else {
                        let raw_samples = wav_tensor.to_vec();
                        let max_val = raw_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        if max_val > 1e-5 {
                            let scale = 0.90 / max_val.max(0.90);
                            raw_samples.iter().map(|s| s * scale).collect()
                        } else {
                            raw_samples
                        }
                    };

                    let wav_bytes = vocoder.encode_wav_bytes(&pcm_samples);
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                    return Ok(format!("data:audio/wav;base64,{}", b64));
                }
            }
            return Err(anyhow!("TTS graph produced empty waveform output."));
        }

        // ─── 3. STT (Speech-to-Text) Execution Graph ───────────────────────
        let t_start = std::time::Instant::now();

        let audio_path = extract_audio_path(prompt)
            .ok_or_else(|| anyhow!("No [AUDIO_INPUT: ...] tag found in prompt"))?;

        let req_lang = extract_parameter(prompt, "LANGUAGE").unwrap_or_else(|| "auto".to_string());
        let req_translate = extract_parameter(prompt, "TRANSLATE_TO").unwrap_or_default();

        let config = AudioConfig::from_model_dir(&self.model_dir);
        let t_pcm = std::time::Instant::now();
        let pcm_samples = load_audio_to_pcm(&audio_path, &config)?;
        println!("⏱️ [BENCHMARK] Step A - PCM Load Time: {:?} (Audio Duration: {:.2}s, Samples: {})", t_pcm.elapsed(), pcm_samples.len() as f32 / config.sample_rate as f32, pcm_samples.len());

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

        let t_mel = std::time::Instant::now();
        let (mel_flat, actual_frames) = compute_log_mel_spectrogram(&pcm_samples, &config);
        println!("⏱️ [BENCHMARK] Step B - Mel Spectrogram Compute Time: {:?} (actual_frames: {}, padded_to: {})", t_mel.elapsed(), actual_frames, config.max_frames);
        let shape_vec = vec![1usize, config.n_mels, config.max_frames];

        let mut real_encoder_hidden_states: Option<(Vec<usize>, Vec<f32>)> = None;

        let t_enc = std::time::Instant::now();
        if let Some(enc_arc) = &self.encoder_session {
            if let Ok(mut enc_guard) = enc_arc.lock() {
                if let Ok(mel_tensor) = Value::from_array((shape_vec.clone(), mel_flat)) {
                    let mut enc_inputs = HashMap::new();
                    enc_inputs.insert("input_features", mel_tensor);

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
        println!("⏱️ [BENCHMARK] Step C - Encoder ONNX Model Inference Time: {:?}", t_enc.elapsed());

        // Dynamic Audio Duration & Multi-Modality Math Algorithm
        let prompt_lower = prompt.to_lowercase();
        let audio_duration_secs = (pcm_samples.len() as f32) / (config.sample_rate as f32);
        
        let dynamic_stt_tokens = ((audio_duration_secs * 1.8) + 6.0).ceil() as usize;
        let dynamic_tts_tokens = ((audio_duration_secs * 6.0) + 16.0).ceil() as usize;
        let dynamic_music_tokens = ((audio_duration_secs * 10.0) + 32.0).ceil() as usize;
        let dynamic_caption_tokens = ((audio_duration_secs * 2.0) + 8.0).ceil() as usize;

        let (max_speech_len, rep_repeat_limit) = if prompt_lower.contains("music_generation") || prompt_lower.contains("text_to_music") || prompt_lower.contains("music_classification") {
            (dynamic_music_tokens.clamp(48, 256), 4usize) // Dynamic Music & Acoustic frame scaling
        } else if prompt_lower.contains("text_to_speech") || prompt_lower.contains("tts") || prompt_lower.contains("voice_conversion") || prompt_lower.contains("audio_enhancement") || prompt_lower.contains("noise_reduction") || prompt_lower.contains("source_separation") {
            (dynamic_tts_tokens.clamp(24, 192), 3usize) // Dynamic Speech Synthesis & Audio Processing scaling
        } else if prompt_lower.contains("audio_classification") || prompt_lower.contains("speaker_identification") || prompt_lower.contains("speaker_verification") || prompt_lower.contains("speaker_diarization") || prompt_lower.contains("emotion_recognition") || prompt_lower.contains("language_identification") || prompt_lower.contains("keyword_spotting") || prompt_lower.contains("wake_word_detection") || prompt_lower.contains("voice_activity_detection") {
            (16usize, 2usize)  // Short sequence for classification / tagging / VAD / KWS
        } else if prompt_lower.contains("audio_captioning") {
            (dynamic_caption_tokens.clamp(16, 64), 2usize)  // Dynamic Captioning sequence length
        } else {
            // Flexible Dynamic Speech-to-Text / Transcription (clamped to realistic spoken token bounds)
            (dynamic_stt_tokens.clamp(16, 48), 2usize)
        };

        let mut speech_tokens: Vec<u32> = Vec::new();
        let mut speech_tokens_set: std::collections::HashSet<u32> = std::collections::HashSet::new();

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

        // Pre-allocate static fallback array & ONNX Value ONCE outside the 224 token loop
        let dummy_shape = vec![1usize, 1500, 1280];
        let dummy_data: Vec<f32> = vec![0.0f32; 1 * 1500 * 1280];
        let dummy_val_opt: Option<Value> = Value::from_array((dummy_shape, dummy_data)).ok().map(|v| v.into());

        let encoder_val_opt: Option<Value> = if let Some((ref hs_shape, ref hs_vec)) = real_encoder_hidden_states {
            Value::from_array((hs_shape.clone(), hs_vec.clone())).ok().map(|v| v.into())
        } else {
            None
        };

        let t_dec_loop = std::time::Instant::now();
        let mut total_step_runs = 0usize;

        let is_decoder_ids_name: Vec<bool> = decoder_input_names.iter().map(|n| {
            let n_str = n.as_str();
            n_str.contains("input_ids") || n_str.contains("decoder_input_ids")
        }).collect();

        for _step in 0..max_speech_len {
            let seq_len = current_decoder_ids.len();
            let input_ids_val = match Value::from_array(([1usize, seq_len], current_decoder_ids.clone())) {
                Ok(v) => v,
                Err(_) => break,
            };
            let input_ids_dyn: Value = input_ids_val.into();

            let mut step_inputs: HashMap<&str, &Value> = HashMap::with_capacity(decoder_input_names.len());

            for (idx, name) in decoder_input_names.iter().enumerate() {
                if is_decoder_ids_name[idx] {
                    step_inputs.insert(name.as_str(), &input_ids_dyn);
                } else {
                    if let Some(ref enc_val) = encoder_val_opt {
                        step_inputs.insert(name.as_str(), enc_val);
                    } else if let Some(ref d_val) = dummy_val_opt {
                        step_inputs.insert(name.as_str(), d_val);
                    }
                }
            }

            let output_tensors = match session_guard.run(step_inputs) {
                Ok(out) => out,
                Err(e) => {
                    eprintln!("ORT decoder run error: {}", e);
                    break;
                }
            };

            let logits_val_ref = output_tensors.get("logits");
            let next_tok = if let Some(ref logits) = logits_val_ref {
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
                    let max_allowed_tok = (step_logits.len().saturating_sub(1)) as u32;
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
                            if speech_tokens_set.contains(&tok_id) {
                                adjusted_val = if adjusted_val < 0.0 {
                                    adjusted_val * rep_penalty
                                } else {
                                    adjusted_val / rep_penalty
                                };
                            }

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

            let is_eot = next_tok == config.end_of_text_token as u32
                || next_tok == 50257
                || next_tok == 50256
                || next_tok == 50362
                || next_tok == 50363
                || next_tok == 50364;

            if is_eot {
                break;
            }

            // Early break on trailing repeated silence / pad tokens (Task-Adaptive Repetition Threshold)
            let len = speech_tokens.len();
            if len >= rep_repeat_limit && speech_tokens[len - 1] == next_tok && speech_tokens[len - 2] == next_tok {
                break;
            }

            speech_tokens.push(next_tok);
            speech_tokens_set.insert(next_tok);
            current_decoder_ids.push(next_tok as i64);
            total_step_runs += 1;

            if let Some(cb) = callback.as_mut() {
                if let Some(tokenizer) = &self.tokenizer {
                    if let Ok(piece) = tokenizer.decode(&[next_tok], false) {
                        if !piece.is_empty() {
                            cb(piece);
                        }
                    }
                }
            }
        }

        let dec_elapsed = t_dec_loop.elapsed();
        let avg_step_ms = if total_step_runs > 0 { dec_elapsed.as_secs_f64() * 1000.0 / (total_step_runs as f64) } else { 0.0 };
        println!("⏱️ [BENCHMARK] Step D - Decoder ONNX Token Loop Time: {:?} (Total Steps: {}, Avg: {:.2}ms/token)", dec_elapsed, total_step_runs, avg_step_ms);
        println!("⏱️ [BENCHMARK] Total Audio Execute Pipeline Time: {:?}", t_start.elapsed());

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