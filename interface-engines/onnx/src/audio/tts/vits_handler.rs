use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Dedicated VITS/Piper TTS Execution Handler
///
/// [FACT] VITS is a single-stage end-to-end model: phoneme IDs → PCM waveform.
/// [FACT] No vocoder needed — VITS generates raw audio directly.
/// [FACT] Input contract (verified from Piper ONNX exports):
///   - `input` or `phoneme_ids`: int64 [1, N] — phoneme token sequence
///   - `input_lengths`: int64 [1] — length of phoneme sequence
///   - `scales`: float32 [3] — [noise_scale, length_scale, noise_w]
///   - `sid` (optional): int64 [1] — speaker ID for multi-speaker models

/// Execute a VITS/Piper ONNX model with proper tensor contracts.
///
/// # Arguments
/// * `session` - Loaded ONNX inference session for the VITS model
/// * `phoneme_ids` - Sequence of phoneme/character IDs from PhonemeMap
/// * `noise_scale` - Controls phoneme-level variation (default: 0.667)
/// * `length_scale` - Controls speaking rate (default: 1.0, lower = faster)
/// * `noise_w` - Controls duration variation (default: 0.8)
/// * `speaker_id` - Optional speaker ID for multi-speaker models
pub fn execute_vits(
    session: &mut Session,
    phoneme_ids: &[i64],
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
    speaker_id: Option<i64>,
) -> Result<Vec<f32>> {
    if phoneme_ids.is_empty() {
        return Err(anyhow!("Cannot synthesize: empty phoneme ID sequence"));
    }

    let seq_len = phoneme_ids.len();
    let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();

    eprintln!(
        "🎙️ [VITS Handler] Executing with {} phoneme IDs, scales=[{}, {}, {}], inputs={:?}",
        seq_len, noise_scale, length_scale, noise_w, input_names
    );

    let mut inputs: HashMap<String, Value> = HashMap::new();

    for name in &input_names {
        let name_lower = name.to_lowercase();

        if name_lower.contains("scales") || name_lower.contains("noise_scale") {
            // VITS scales tensor: [noise_scale, length_scale, noise_w]
            let scales_val = Value::from_array(([3usize], vec![noise_scale, length_scale, noise_w]))
                .map_err(|e| anyhow!("Failed to create scales tensor: {}", e))?;
            inputs.insert(name.clone(), scales_val.into());
        } else if name_lower.contains("length") || name_lower == "input_lengths" {
            // Input sequence length
            let len_val = Value::from_array(([1usize], vec![seq_len as i64]))
                .map_err(|e| anyhow!("Failed to create input_lengths tensor: {}", e))?;
            inputs.insert(name.clone(), len_val.into());
        } else if name_lower == "sid" || name_lower.contains("speaker") {
            // Speaker ID for multi-speaker models
            let sid = speaker_id.unwrap_or(0);
            let sid_val = Value::from_array(([1usize], vec![sid]))
                .map_err(|e| anyhow!("Failed to create speaker_id tensor: {}", e))?;
            inputs.insert(name.clone(), sid_val.into());
        } else {
            // Primary input: phoneme IDs tensor [1, seq_len]
            let ids_val = Value::from_array(([1usize, seq_len], phoneme_ids.to_vec()))
                .map_err(|e| anyhow!("Failed to create phoneme_ids tensor: {}", e))?;
            inputs.insert(name.clone(), ids_val.into());
        }
    }

    if inputs.is_empty() {
        return Err(anyhow!("VITS model has no recognized inputs. Expected: input/phoneme_ids, input_lengths, scales"));
    }

    let outputs = session
        .run(inputs)
        .map_err(|e| anyhow!("VITS ONNX session execution failed: {}", e))?;

    // Extract PCM waveform from first output tensor
    if let Some(output_val) = outputs.values().next() {
        if let Ok((_shape, tensor)) = output_val.try_extract_tensor::<f32>() {
            let pcm = tensor.to_vec();
            eprintln!(
                "🎙️ [VITS Handler] Output PCM: {} samples ({:.2}s at 22050Hz)",
                pcm.len(),
                pcm.len() as f32 / 22050.0
            );
            if pcm.is_empty() {
                return Err(anyhow!("VITS model produced empty audio output"));
            }
            return Ok(pcm);
        }
    }

    Err(anyhow!("Failed to extract PCM waveform from VITS output tensor"))
}
