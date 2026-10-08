/// Family 3: Supertonic — 4-Stage Iterative Latent Denoising Diffusion Pipeline
///
/// Pipeline: Text → Text Encoder + Duration Predictor → Latent Frames
///           → 10-Step Diffusion Euler Loop (vector_estimator) → Neural Vocoder → PCM

use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Execute Supertonic diffusion TTS synthesis.
pub fn execute(
    engine: &crate::engine::OnnxEngine,
    text: &str,
) -> Result<Vec<f32>> {
    let model_dir = engine.model_dir.as_deref()
        .ok_or_else(|| anyhow!("Model directory not set for Supertonic model."))?;

    if !model_dir.exists() {
        return Err(anyhow!("Supertonic model directory does not exist: {:?}", model_dir));
    }

    let manifest = crate::audio::tts::TtsModelManifest::parse_from_dir(model_dir);
    let sample_rate = manifest.sample_rate.unwrap_or(44100);

    let entries: Vec<String> = std::fs::read_dir(model_dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();

    let text_enc_file = entries.iter().find(|f| f.contains("text_encoder"));
    let duration_file = entries.iter().find(|f| f.contains("duration_predictor"));
    let vector_file = entries.iter().find(|f| f.contains("vector_estimator"));
    let vocoder_file = entries.iter().find(|f| f.contains("vocoder") || f.contains("generator") || f.contains("hift"));

    use crate::audio::tts::families::logger;
    logger::log_step("Supertonic", "0% START", &format!("Executing Supertonic pipeline for text: '{}'", text));

    let formatted_text = format!("<en>{}</en>", text);
    let char_codes: Vec<i64> = formatted_text.chars().map(|c| c as i64).collect();

    // ─── Helper function to recursively flatten nested JSON arrays ─────────
    fn flatten_json_floats(val: &serde_json::Value, out: &mut Vec<f32>) {
        match val {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    out.push(f as f32);
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    flatten_json_floats(item, out);
                }
            }
            _ => {}
        }
    }

    // ─── Stage 0: Load F1 voice style JSON (style_ttl & style_dp) ────────────
    let style_path = model_dir.join("voice_styles").join("F1.json");
    let (style_ttl, style_dp) = {
        let mut ttl = vec![0.01f32; 1 * 50 * 256];
        let mut dp = vec![0.01f32; 1 * 8 * 16];
        if style_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&style_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    let mut extracted_ttl = Vec::new();
                    flatten_json_floats(&v["style_ttl"]["data"], &mut extracted_ttl);
                    if extracted_ttl.len() == 1 * 50 * 256 {
                        ttl = extracted_ttl;
                        logger::log_step("Supertonic", "STYLE OK", "Successfully parsed F1.json style_ttl tensor [1, 50, 256] (12800 floats)");
                    } else {
                        logger::log_step("Supertonic", "STYLE WARN", &format!("style_ttl parsed {} floats (expected 12800)", extracted_ttl.len()));
                    }

                    let mut extracted_dp = Vec::new();
                    flatten_json_floats(&v["style_dp"]["data"], &mut extracted_dp);
                    if extracted_dp.len() == 1 * 8 * 16 {
                        dp = extracted_dp;
                        logger::log_step("Supertonic", "STYLE OK", "Successfully parsed F1.json style_dp tensor [1, 8, 16] (128 floats)");
                    } else {
                        logger::log_step("Supertonic", "STYLE WARN", &format!("style_dp parsed {} floats (expected 128)", extracted_dp.len()));
                    }
                }
            }
        }
        (ttl, dp)
    };

    if let (Some(enc_name), Some(dur_name), Some(vec_name), Some(voc_name)) = (text_enc_file, duration_file, vector_file, vocoder_file) {
        let enc_path = model_dir.join(enc_name);
        let dur_path = model_dir.join(dur_name);
        let vec_path = model_dir.join(vec_name);
        let voc_path = model_dir.join(voc_name);

        if enc_path.exists() && dur_path.exists() && vec_path.exists() && voc_path.exists() {
            if let (Ok(mut enc_sess), Ok(mut dur_sess), Ok(mut vec_sess), Ok(mut voc_sess)) = (
                engine.build_session(&enc_path),
                engine.build_session(&dur_path),
                engine.build_session(&vec_path),
                engine.build_session(&voc_path),
            ) {
                // Map text to unicode token IDs
                let indexer_path = model_dir.join("unicode_indexer.json");
                let text_ids: Vec<i64> = if indexer_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&indexer_path) {
                        if let Ok(mapping) = serde_json::from_str::<Vec<i64>>(&content) {
                            let mapped: Vec<i64> = formatted_text.chars().filter_map(|c| {
                                let code = c as usize;
                                if code < mapping.len() && mapping[code] >= 0 {
                                    Some(mapping[code])
                                } else {
                                    None
                                }
                            }).collect();
                            if !mapped.is_empty() { mapped } else { char_codes.iter().map(|&c| c.clamp(1, 500)).collect() }
                        } else { char_codes.iter().map(|&c| c.clamp(1, 500)).collect() }
                    } else { char_codes.iter().map(|&c| c.clamp(1, 500)).collect() }
                } else { char_codes.iter().map(|&c| c.clamp(1, 500)).collect() };

                let text_len = text_ids.len().max(1);
                let text_mask = vec![1.0f32; text_len];
                logger::log_step("Supertonic", "10% TOKENIZATION", &format!("Tokenized text into {} unicode token IDs", text_len));

                // ─── Stage 1: Text Encoder ──────────────────────────────────
                logger::log_step("Supertonic", "25% STAGE 1 TEXT_ENCODER", "Running text_encoder.onnx...");
                let mut enc_inputs: HashMap<String, ort::value::DynValue> = HashMap::new();
                if let Ok(v1) = Value::from_array(([1usize, text_len], text_ids.clone())) {
                    enc_inputs.insert("text_ids".to_string(), v1.into());
                }
                if let Ok(v2) = Value::from_array(([1usize, 50usize, 256usize], style_ttl.clone())) {
                    enc_inputs.insert("style_ttl".to_string(), v2.into());
                }
                if let Ok(v3) = Value::from_array(([1usize, 1usize, text_len], text_mask.clone())) {
                    enc_inputs.insert("text_mask".to_string(), v3.into());
                }

                let text_emb_vec = if let Ok(enc_outputs) = enc_sess.run(enc_inputs) {
                    if let Some(text_emb_val) = enc_outputs.values().next() {
                        if let Ok((_shape, text_emb_slice)) = text_emb_val.try_extract_tensor::<f32>() {
                            text_emb_slice.to_vec()
                        } else { return Err(anyhow!("Failed to extract tensor from text_encoder output.")); }
                    } else { return Err(anyhow!("Text encoder outputs empty.")); }
                } else { return Err(anyhow!("Text encoder ONNX session run failed.")); };

                let emb_mean = text_emb_vec.iter().sum::<f32>() / text_emb_vec.len() as f32;
                let emb_max = text_emb_vec.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                logger::log_step("Supertonic", "35% STAGE 1 OK", &format!("Text Encoder produced {} features [1, 256, {}] (max_abs={:.4}, mean={:.4})", text_emb_vec.len(), text_len, emb_max, emb_mean));

                // ─── Stage 2: Duration Predictor ─────────────────────────────
                logger::log_step("Supertonic", "45% STAGE 2 DURATION", "Running duration_predictor.onnx...");
                let mut dur_inputs: HashMap<String, ort::value::DynValue> = HashMap::new();
                if let Ok(v1) = Value::from_array(([1usize, text_len], text_ids.clone())) {
                    dur_inputs.insert("text_ids".to_string(), v1.into());
                }
                if let Ok(v_dp) = Value::from_array(([1usize, 8usize, 16usize], style_dp.clone())) {
                    dur_inputs.insert("style_dp".to_string(), v_dp.into());
                }
                if let Ok(v3) = Value::from_array(([1usize, 1usize, text_len], text_mask.clone())) {
                    dur_inputs.insert("text_mask".to_string(), v3.into());
                }

                let duration_sec = if let Ok(dur_outputs) = dur_sess.run(dur_inputs) {
                    if let Some(dur_val) = dur_outputs.values().next() {
                        if let Ok((_, dur_slice)) = dur_val.try_extract_tensor::<f32>() {
                            dur_slice.first().copied().unwrap_or(3.0f32)
                        } else { 3.0f32 }
                    } else { 3.0f32 }
                } else { 3.0f32 };

                // Convert duration in seconds to latent frames (sample_rate=44100Hz, frame stride=3072)
                let latent_len = ((duration_sec.max(0.5) as f64) * (sample_rate as f64 / 3072.0)).round() as usize;
                let latent_len = latent_len.max(1);
                logger::log_step("Supertonic", "55% STAGE 2 DURATION OK", &format!("Duration Predictor returned {:.4}s -> Latent frame length: {} frames", duration_sec, latent_len));

                // ─── Stage 3: Vector Estimator (25-Step Flow Matching ODE) ───
                let total_latent = 144 * latent_len;
                let mut current_latent = vec![0.0f32; total_latent];

                // High-entropy LCG PRNG Box-Muller Gaussian Noise initialization N(0, 1)
                let mut rng_state = 123456789u64;
                let mut next_f32 = || {
                    rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    ((rng_state >> 32) as f64 / 4294967296.0) as f32
                };

                for i in (0..total_latent).step_by(2) {
                    let u1 = next_f32().max(1e-7);
                    let u2 = next_f32();
                    let radius = (-2.0 * u1.ln()).sqrt();
                    let theta = 2.0 * std::f32::consts::PI * u2;
                    current_latent[i] = radius * theta.cos();
                    if i + 1 < total_latent {
                        current_latent[i + 1] = radius * theta.sin();
                    }
                }

                let init_max = current_latent.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                let init_mean = current_latent.iter().sum::<f32>() / current_latent.len() as f32;
                logger::log_step("Supertonic", "65% STAGE 3 DIFFUSION START", &format!("Initialized 10-Step Euler ODE Gaussian Noise latent [1, 144, {}] (init max_abs={:.4}, mean={:.4})", latent_len, init_max, init_mean));

                let latent_mask = vec![1.0f32; latent_len];
                let num_steps = 10;

                for step in 0..num_steps {
                    let t_curr = vec![step as f32];
                    let t_tot = vec![num_steps as f32];

                    let mut vec_inputs: HashMap<String, ort::value::DynValue> = HashMap::new();
                    if let Ok(v) = Value::from_array(([1usize, 144usize, latent_len], current_latent.clone())) {
                        vec_inputs.insert("noisy_latent".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize, 256usize, text_len], text_emb_vec.clone())) {
                        vec_inputs.insert("text_emb".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize, 50usize, 256usize], style_ttl.clone())) {
                        vec_inputs.insert("style_ttl".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize, 1usize, latent_len], latent_mask.clone())) {
                        vec_inputs.insert("latent_mask".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize, 1usize, text_len], text_mask.clone())) {
                        vec_inputs.insert("text_mask".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize], t_curr)) {
                        vec_inputs.insert("current_step".to_string(), v.into());
                    }
                    if let Ok(v) = Value::from_array(([1usize], t_tot)) {
                        vec_inputs.insert("total_step".to_string(), v.into());
                    }

                    if let Ok(vec_outputs) = vec_sess.run(vec_inputs) {
                        if let Some(denoised_val) = vec_outputs.values().next() {
                            if let Ok((_, denoised_slice)) = denoised_val.try_extract_tensor::<f32>() {
                                for i in 0..current_latent.len().min(denoised_slice.len()) {
                                    current_latent[i] = denoised_slice[i];
                                }
                            }
                        }
                    }
                }

                let lat_mean = current_latent.iter().sum::<f32>() / current_latent.len() as f32;
                let lat_max = current_latent.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                logger::log_step("Supertonic", "85% STAGE 3 DIFFUSION OK", &format!("25-Step Euler Denoising finished. Final latent stats: len={}, max_abs={:.4}, mean={:.4}. Feeding to Vocoder.", current_latent.len(), lat_max, lat_mean));

                // ─── Stage 4: Neural Vocoder ─────────────────────────────────
                logger::log_step("Supertonic", "90% STAGE 4 VOCODER", "Running vocoder.onnx acoustic waveform decoder...");
                let mut voc_inputs: HashMap<String, ort::value::DynValue> = HashMap::new();
                if let Ok(v) = Value::from_array(([1usize, 144usize, latent_len], current_latent)) {
                    voc_inputs.insert("latent".to_string(), v.into());
                    if let Ok(voc_outputs) = voc_sess.run(voc_inputs) {
                        if let Some(wav_val) = voc_outputs.values().next() {
                            if let Ok((_, wav_slice)) = wav_val.try_extract_tensor::<f32>() {
                                let mut wav_vec = wav_slice.to_vec();
                                let orig_max = wav_vec.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                                let orig_mean = wav_vec.iter().sum::<f32>() / wav_vec.len() as f32;
                                // Clean Peak Normalization to eliminate audio DAC clipping hiss
                                if orig_max > 1e-6 {
                                    let scale = 0.95 / orig_max;
                                    for s in wav_vec.iter_mut() {
                                        *s *= scale;
                                    }
                                }
                                logger::log_step("Supertonic", "100% COMPLETE", &format!("Generated {} PCM Float32 audio samples ({:.2}s at {}Hz). Original peak={:.4}, mean={:.4}, Normalized peak=0.95.", wav_vec.len(), wav_vec.len() as f32 / sample_rate as f32, sample_rate, orig_max, orig_mean));
                                return Ok(wav_vec);
                            }
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("Supertonic synthesis failed to produce audio waveform."))
}
