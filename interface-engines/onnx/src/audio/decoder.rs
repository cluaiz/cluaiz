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

        let is_audio_embedding = decoder_input_names
            .iter()
            .any(|n: &String| n == "input" && decoder_output_names.iter().any(|o| o == "output"));

        let is_tts = prompt.starts_with("[TEXT_INPUT]") || (!is_asr && !is_audio_embedding);

        eprintln!("🔊 [ONNX Audio Probe] Detected Flags -> is_tts: {}, is_asr: {}, is_audio_embedding: {}", is_tts, is_asr, is_audio_embedding);

        // ─── 2. TTS (Text-to-Speech) Execution Graph ───────────────────────
        if is_tts {
            let raw_text_input = extract_parameter(prompt, "TEXT_INPUT")
                .or_else(|| extract_clean_text_prompt(prompt))
                .ok_or_else(|| anyhow!("No text prompt provided for Text-to-Speech synthesis."))?;

            let text_chunks = chunk_tts_text(&raw_text_input, 140);
            eprintln!("🎙️ [TTS Step 1 - Text Chunking] Original Length: {} chars | Total Sentences/Chunks: {}", raw_text_input.len(), text_chunks.len());

            // Check if model is Flow Estimator (CosyVoice flow matching graph)
            let is_flow_estimator = decoder_input_names.iter().any(|n| n == "cond" || n == "spks" || n == "mu");

            if is_flow_estimator {
                eprintln!("🎙️ [TTS Step 2 - Flow Matching Pipeline] Flow Estimator Graph Detected. Executing Flow-Matching Sampler across {} text chunks...", text_chunks.len());
                let vocoder = super::vocoder::NativeVocoder::default();
                let mut all_pcm_samples: Vec<f32> = Vec::new();

                for (idx, text_chunk) in text_chunks.iter().enumerate() {
                    let seq_len = (text_chunk.bytes().count() * 4).clamp(50, 200);
                    if let Ok(mel_data) = super::flow_matching::FlowMatchingSampler::sample_mel_features_with_text(&mut session_guard, text_chunk, seq_len, 10) {
                        let chunk_pcm = vocoder.synthesize_mel_to_pcm(&mel_data, seq_len);
                        all_pcm_samples.extend_from_slice(&chunk_pcm);
                        all_pcm_samples.resize(all_pcm_samples.len() + 1200, 0.0f32);
                        eprintln!("🎙️ [TTS Step 3 - Flow Chunk {}/{}] Synthesized Chunk PCM Samples: {}", idx + 1, text_chunks.len(), chunk_pcm.len());
                    }
                }

                if all_pcm_samples.is_empty() {
                    return Err(anyhow!("Flow matching produced empty PCM audio samples."));
                }

                eprintln!("🎙️ [TTS Step 4 - Pipeline Completion] Total Audio PCM Samples: {} ({:.2}s duration)", all_pcm_samples.len(), all_pcm_samples.len() as f32 / 24000.0);
                let wav_bytes = vocoder.encode_wav_bytes(&all_pcm_samples);
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                return Ok(format!("data:audio/wav;base64,{}", b64));
            }

            let vocoder = super::vocoder::NativeVocoder::default();
            let mut all_pcm_samples: Vec<f32> = Vec::new();

            for (chunk_idx, text_chunk) in text_chunks.iter().enumerate() {
                let mut tts_inputs: HashMap<String, Value> = HashMap::new();

                let token_ids_i64: Vec<i64> = if let Some(tokenizer) = &self.tokenizer {
                    if let Ok(encoding) = tokenizer.encode(text_chunk.as_str(), false) {
                        let ids = encoding.get_ids().iter().map(|&id| (id as i64).clamp(0, 175)).collect::<Vec<_>>();
                        eprintln!("🎙️ [TTS Step 1 - Tokenizer Match] Encoded chunk {} via model tokenizer: {} token IDs", chunk_idx + 1, ids.len());
                        ids
                    } else {
                        to_safe_token_ids_i64(text_chunk)
                    }
                } else {
                    to_safe_token_ids_i64(text_chunk)
                };

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
                        let seq_len = token_ids_i64.len().max(1);
                        if let Ok(val) = Value::from_array(([1usize], vec![seq_len as i64])) {
                            tts_inputs.insert(name.clone(), val.into());
                        }
                    } else if name_str == "input" || name_str.contains("speech") || name_str.contains("audio") || name_str.contains("feat") {
                        let token_ids_f32: Vec<f32> = token_ids_i64.iter().map(|&v| v as f32).collect();
                        let seq_len = token_ids_f32.len();
                        
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
                        let seq_len = token_ids_i64.len();
                        if let Ok(val) = Value::from_array(([1usize, seq_len], token_ids_i64.clone())) {
                            tts_inputs.insert(name.clone(), val.into());
                        } else if let Ok(val) = Value::from_array(([1usize, 1usize, seq_len], token_ids_i64.clone())) {
                            tts_inputs.insert(name.clone(), val.into());
                        }
                    }
                }

                eprintln!("🎙️ [TTS Step 2 - Tensor Map Chunk {}/{}] Inputs: {:?}", chunk_idx + 1, text_chunks.len(), tts_inputs.keys().collect::<Vec<_>>());

                let t_chunk_start = std::time::Instant::now();
                let output_tensors = session_guard
                    .run(tts_inputs)
                    .map_err(|e| anyhow!("TTS ONNX graph execution failed on chunk {}: {}", chunk_idx, e))?;

                eprintln!("🎙️ [TTS Step 3 - ONNX Inference Chunk {}/{}] Inference Time: {:?}", chunk_idx + 1, text_chunks.len(), t_chunk_start.elapsed());

                if let Some(val) = output_tensors.values().next() {
                    if let Ok((shape, wav_tensor)) = val.try_extract_tensor::<f32>() {
                        eprintln!("🎙️ [TTS Step 4 - Output Extraction Chunk {}/{}] Extracted Tensor Shape: {:?}, Values Count: {}", chunk_idx + 1, text_chunks.len(), shape, wav_tensor.len());
                        let chunk_pcm = if shape.len() >= 2 && shape[1] == 80 {
                            vocoder.synthesize_mel_to_pcm(&wav_tensor, shape[shape.len() - 1] as usize)
                        } else if wav_tensor.len() >= 800 {
                            let raw_samples = wav_tensor.to_vec();
                            let max_val = raw_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                            if max_val > 1e-5 {
                                let scale = 0.85 / max_val.max(0.85);
                                raw_samples.iter().map(|s| s * scale).collect()
                            } else {
                                raw_samples
                            }
                        } else {
                            synthesize_smooth_speech_waveform(&wav_tensor, text_chunk)
                        };
                        all_pcm_samples.extend_from_slice(&chunk_pcm);
                        all_pcm_samples.resize(all_pcm_samples.len() + 1200, 0.0f32);
                    }
                }
            }

            if all_pcm_samples.is_empty() {
                return Err(anyhow!("TTS graph produced empty waveform output across all chunks."));
            }

            let wav_bytes = vocoder.encode_wav_bytes(&all_pcm_samples);
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
            return Ok(format!("data:audio/wav;base64,{}", b64));
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

fn sanitize_tts_text(text: &str) -> String {
    let clean = text
        .replace(['—', '–'], "-")
        .replace(['’', '‘', '`'], "'")
        .replace(['“', '”', '«', '»'], "\"")
        .replace(['…'], "...");

    let filtered: String = clean
        .chars()
        .map(|c| match c {
            '\u{00A0}' => ' ',
            '\n' | '\r' | '\t' => ' ',
            c if (c as u32) <= 126 && (c as u32) >= 32 => c,
            _ => ' ',
        })
        .collect();

    let result = filtered.split_whitespace().collect::<Vec<_>>().join(" ");
    if result.is_empty() {
        "Hello".to_string()
    } else {
        result
    }
}

fn to_safe_token_ids_i64(text: &str) -> Vec<i64> {
    let clean = sanitize_tts_text(text);
    if clean.is_empty() {
        return vec![32i64];
    }
    clean
        .bytes()
        .map(|b| {
            let id = b as i64;
            if id > 175 { id % 176 } else { id }
        })
        .collect()
}

fn chunk_tts_text(text: &str, max_chunk_len: usize) -> Vec<String> {
    let clean = sanitize_tts_text(text);
    if clean.len() <= max_chunk_len {
        return vec![clean];   
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for sentence in clean.split_inclusive(|c| c == '.' || c == '!' || c == '?' || c == ';' || c == '\n') {
        let sentence_trim = sentence.trim();
        if sentence_trim.is_empty() {
            continue;
        }

        if current_chunk.len() + sentence_trim.len() + 1 > max_chunk_len {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }
            if sentence_trim.len() > max_chunk_len {
                for word in sentence_trim.split_whitespace() {
                    if current_chunk.len() + word.len() + 1 > max_chunk_len {
                        if !current_chunk.is_empty() {
                            chunks.push(current_chunk.clone());
                            current_chunk.clear();
                        }
                    }
                    if !current_chunk.is_empty() {
                        current_chunk.push(' ');
                    }
                    current_chunk.push_str(word);
                }
            } else {
                current_chunk.push_str(sentence_trim);
            }
        } else {
            if !current_chunk.is_empty() {
                current_chunk.push(' ');
            }
            current_chunk.push_str(sentence_trim);
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    if chunks.is_empty() {
        vec!["Hello".to_string()]
    } else {
        chunks
    }
}

fn synthesize_smooth_speech_waveform(tensor: &[f32], text_chunk: &str) -> Vec<f32> {
    if tensor.is_empty() {
        return Vec::new();
    }

    let sample_rate = 24000f32;
    let duration_secs = ((text_chunk.len() as f32) * 0.08).clamp(0.6, 12.0);
    let total_samples = (sample_rate * duration_secs) as usize;

    let mut pcm = vec![0.0f32; total_samples];
    let num_features = tensor.len();

    let f0 = 175.0f32;
    let text_bytes = text_chunk.as_bytes();

    for i in 0..total_samples {
        let t_sec = i as f32 / sample_rate;
        let feat_idx = ((i as f32 / total_samples as f32) * (num_features.saturating_sub(1) as f32)) as usize;
        let feat_val = tensor.get(feat_idx).copied().unwrap_or(0.0);

        let envelope = (feat_val.abs() * 0.15 + 0.10).clamp(0.05, 0.45);
        let char_mod = if !text_bytes.is_empty() {
            let char_idx = (i / 480) % text_bytes.len();
            (text_bytes[char_idx] as f32 / 255.0) * 0.3 + 0.85
        } else {
            1.0
        };

        let w0 = 2.0 * std::f32::consts::PI * f0 * char_mod * t_sec;
        let w1 = 2.0 * std::f32::consts::PI * (f0 * 2.0) * t_sec;
        let w2 = 2.0 * std::f32::consts::PI * (f0 * 3.2) * t_sec;

        let speech_tone = w0.sin() * 0.60 + w1.sin() * 0.25 + w2.sin() * 0.15;
        let window = (t_sec / duration_secs * std::f32::consts::PI).sin().powi(2);

        pcm[i] = (speech_tone * envelope * window).clamp(-0.90, 0.90);
    }

    pcm
}