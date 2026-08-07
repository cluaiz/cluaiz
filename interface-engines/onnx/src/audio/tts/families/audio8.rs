/// Family 4: Audio8 — 3-Stage Auto-Regressive Codec Transformer
///
/// Pipeline: Text → Tokenizer → Slow AR (coarse tokens) → Fast AR (fine codebook)
///           → Codec Decoder → PCM Waveform
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Execute Audio8 Codec-LM TTS synthesis.
pub fn execute(engine: &crate::engine::OnnxEngine, text: &str) -> Result<Vec<f32>> {
    let model_dir = engine
        .model_dir
        .as_deref()
        .ok_or_else(|| anyhow!("Model directory not set for Audio8 model."))?;

    if !model_dir.exists() {
        return Err(anyhow!(
            "Audio8 model directory does not exist: {:?}",
            model_dir
        ));
    }

    let manifest = crate::audio::tts::TtsModelManifest::parse_from_dir(model_dir);
    let num_codebooks = manifest.num_codebooks.unwrap_or(10);
    let sample_rate = manifest.sample_rate.unwrap_or(44100);

    eprintln!(
        "📖 [Audio8 Handler] Manifest parsed: sample_rate={}Hz, num_codebooks={}",
        sample_rate, num_codebooks
    );

    let entries: Vec<String> = std::fs::read_dir(model_dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();

    let slow_ar_file = entries.iter().find(|f| f.contains("slow_ar"));
    let _fast_ar_file = entries.iter().find(|f| f.contains("fast_ar"));
    let codec_file = entries
        .iter()
        .find(|f| f.contains("codec_decoder") || f.contains("decoder"));

    if slow_ar_file.is_none() {
        return Err(anyhow!(
            "PackageContractException: Audio8 Codec-LM model missing required 'slow_ar.onnx' graph."
        ));
    }

    eprintln!(
        "🎙️ [Audio8 Handler] Executing 3-Stage Codec-LM Pipeline for text: '{}'",
        text
    );

    let manifest_path = model_dir.join("runtime_manifest.json");
    let semantic_offset: i64 = if manifest_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                v.get("semantic_begin_id")
                    .and_then(|id| id.as_i64())
                    .unwrap_or(151678)
            } else {
                151678
            }
        } else {
            151678
        }
    } else {
        151678
    };

    let tokenizer_path = model_dir.join("tokenizer.json");
    let token_ids: Vec<i64> = if tokenizer_path.exists() {
        if let Ok(tok) = tokenizers::Tokenizer::from_file(&tokenizer_path) {
            if let Ok(encoding) = tok.encode(text, false) {
                let ids: Vec<i64> = encoding
                    .get_ids()
                    .iter()
                    .map(|&id| (id as i64) + semantic_offset)
                    .collect();
                eprintln!(
                    "📖 [Audio8 Tokenizer] Encoded '{}' into {} tokens with semantic offset {}.",
                    text,
                    ids.len(),
                    semantic_offset
                );
                ids
            } else {
                text.chars().map(|c| (c as i64) + semantic_offset).collect()
            }
        } else {
            text.chars().map(|c| (c as i64) + semantic_offset).collect()
        }
    } else {
        text.chars().map(|c| (c as i64) + semantic_offset).collect()
    };

    let seq_len = token_ids.len().max(1);

    use super::logger;
    logger::log_step(
        "Audio8",
        "0% START",
        &format!(
            "Received text input: '{}' (token_count={})",
            text,
            token_ids.len()
        ),
    );
    logger::log_step(
        "Audio8",
        "15% TOKENIZATION",
        &format!("Tokenized text into {} IDs", seq_len),
    );

    let mut generated_codes: Vec<i64> = Vec::new();

    // 🎯 Attempt Stage 1: Slow AR Execution if graph exists
    if let Some(slow_name) = slow_ar_file {
        let slow_path = model_dir.join(slow_name);
        if slow_path.exists() {
            logger::log_step(
                "Audio8",
                "30% SLOW_AR",
                &format!(
                    "Executing Slow AR graph: {:?}",
                    slow_path.file_name().unwrap()
                ),
            );
            if let Ok(mut slow_sess) = Session::builder()?.commit_from_file(&slow_path) {
                let mut slow_inputs: HashMap<String, Value> = HashMap::new();
                for input in slow_sess.inputs() {
                    let name = input.name().to_string();
                    let lower = name.to_lowercase();
                    if lower.contains("mask") {
                        if let Ok(v) = Value::from_array(([1usize, seq_len], vec![1i64; seq_len])) {
                            slow_inputs.insert(name, v.into());
                        }
                    } else if lower.contains("pos") {
                        let pos: Vec<i64> = (0..seq_len as i64).collect();
                        if let Ok(v) = Value::from_array(([1usize, seq_len], pos)) {
                            slow_inputs.insert(name, v.into());
                        }
                    } else {
                        if let Ok(v) = Value::from_array(([1usize, seq_len], token_ids.clone())) {
                            slow_inputs.insert(name, v.into());
                        }
                    }
                }
                match slow_sess.run(slow_inputs) {
                    Ok(slow_outputs) => {
                        for (_, val) in slow_outputs.iter() {
                            if let Ok((_shape, tensor)) = val.try_extract_tensor::<i64>() {
                                generated_codes = tensor.to_vec();
                                break;
                            } else if let Ok((_shape, tensor)) = val.try_extract_tensor::<i32>() {
                                generated_codes = tensor.iter().map(|&x| x as i64).collect();
                                break;
                            }
                        }
                        logger::log_step(
                            "Audio8",
                            "45% SLOW_AR OK",
                            &format!("Produced {} coarse token codes", generated_codes.len()),
                        );
                    }
                    Err(e) => {
                        logger::log_step(
                            "Audio8",
                            "ERR SLOW_AR FAIL",
                            &format!("slow_ar ONNX graph execution error: {}", e),
                        );
                    }
                }
            }
        }
    }

    if generated_codes.is_empty() {
        logger::log_step(
            "Audio8",
            "ERR ABORT",
            "Slow AR generation failed to produce codebook tokens. Aborting.",
        );
        return Err(anyhow!("Audio8 Slow AR generation failed to produce codebook tokens. Aborting codec decoder to prevent static noise."));
    }

    // 🎯 Stage 2: Fast AR (Fine Codebook Expansion)
    let frames = generated_codes.len(); // Slow AR outputs [1, frames]
    if let Some(fast_name) = _fast_ar_file {
        let fast_path = model_dir.join(fast_name);
        if fast_path.exists() {
            logger::log_step(
                "Audio8",
                "60% FAST_AR",
                "Executing Fast AR fine codebook expansion...",
            );
            if let Ok(mut fast_sess) = Session::builder()?.commit_from_file(&fast_path) {
                let mut fast_inputs: HashMap<String, Value> = HashMap::new();
                let input_name = fast_sess
                    .inputs()
                    .first()
                    .map(|i| i.name().to_string())
                    .unwrap_or_else(|| "codes".to_string());
                if let Ok(val) = Value::from_array(([1usize, frames], generated_codes.clone())) {
                    fast_inputs.insert(input_name, val.into());
                    if let Ok(fast_outputs) = fast_sess.run(fast_inputs) {
                        if let Some(out_val) = fast_outputs.values().next() {
                            if let Ok((shape, tensor)) = out_val.try_extract_tensor::<i64>() {
                                generated_codes = tensor.to_vec();
                                logger::log_step(
                                    "Audio8",
                                    "75% FAST_AR OK",
                                    &format!(
                                        "Produced {} fine tokens shape {:?}",
                                        generated_codes.len(),
                                        shape
                                    ),
                                );
                            } else if let Ok((shape, tensor)) = out_val.try_extract_tensor::<i32>()
                            {
                                generated_codes = tensor.iter().map(|&x| x as i64).collect();
                                logger::log_step(
                                    "Audio8",
                                    "75% FAST_AR OK",
                                    &format!(
                                        "Produced {} fine tokens shape {:?}",
                                        generated_codes.len(),
                                        shape
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // 🎯 Step 3: Codec Decoder PCM Synthesis
    if let Some(codec_name) = codec_file {
        let codec_path = model_dir.join(codec_name);
        if codec_path.exists() {
            if let Ok(mut codec_sess) = Session::builder()?.commit_from_file(&codec_path) {
                let input_name = codec_sess
                    .inputs()
                    .first()
                    .map(|i| i.name().to_string())
                    .unwrap_or_else(|| "codes".to_string());

                let mut expanded_codes = Vec::with_capacity(10 * frames);
                for book in 0..10 {
                    for f in 0..frames {
                        let code = generated_codes.get(f).copied().unwrap_or(0);
                        expanded_codes.push((code + book as i64) % 1024);
                    }
                }

                if let Ok(val) = Value::from_array(([1usize, 10usize, frames], expanded_codes)) {
                    let mut codec_inputs: HashMap<String, Value> = HashMap::new();
                    codec_inputs.insert(input_name, val.into());
                    if let Ok(codec_outputs) = codec_sess.run(codec_inputs) {
                        if let Some(out_val) = codec_outputs.values().next() {
                            if let Ok((_shape, tensor)) = out_val.try_extract_tensor::<f32>() {
                                let pcm = tensor.to_vec();
                                if !pcm.is_empty() {
                                    eprintln!("🎙️ [Audio8 Stage 3/3] Real Codec Decoder Output PCM: {} samples", pcm.len());
                                    return Ok(pcm);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!(
        "Audio8 codec decoder execution failed to produce acoustic PCM samples."
    ))
}
