/// Family 1: Piper/VITS — Single-Stage End-to-End Variational Inference
///
/// Pipeline: Text → Phonemizer → Token IDs → model.onnx → PCM Float32 Waveform
///
/// Required Package:
/// - model.onnx (or model_q4.onnx)
/// - config.json (phoneme_id_map, sample_rate, noise_scale)

use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Execute VITS/Piper TTS synthesis with proper tensor contracts.
pub fn execute(
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

    // Intersperse BOS/EOS and the sequence with exactly one zero between all tokens.
    // The input `phoneme_ids` is already interspersed with 0 between matched tokens by PhonemeMap.
    // We prepended 0, insert 0 after BOS (first element), and append 0 after EOS (last element),
    // matching exactly the expected VITS/Piper alignment format.
    let mut padded_ids: Vec<i64> = Vec::with_capacity(phoneme_ids.len() + 3);
    if !phoneme_ids.is_empty() {
        padded_ids.push(0);              // Leading PAD (0)
        padded_ids.push(phoneme_ids[0]); // BOS (usually 1)
        padded_ids.push(0);              // PAD (0) after BOS
        for &id in &phoneme_ids[1..] {
            padded_ids.push(id);
        }
        if padded_ids.last() != Some(&0) {
            padded_ids.push(0);
        }
    }
    let seq_len = padded_ids.len();
    eprintln!("DEBUG: phoneme_ids = {:?}", phoneme_ids);
    eprintln!("DEBUG: padded_ids = {:?}", &padded_ids[..padded_ids.len().min(30)]);
    let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
    use super::logger;
    logger::log_step("VitsPiper", "0% START", &format!("Executing VITS with {} raw phoneme IDs", phoneme_ids.len()));
    logger::log_step("VitsPiper", "30% PADDING", &format!("Constructed {} padded phoneme IDs sequence with scales=[{}, {}, {}]", seq_len, noise_scale, length_scale, noise_w));

    let mut inputs: HashMap<String, Value> = HashMap::new();

    for name in &input_names {
        let name_lower = name.to_lowercase();

        if name_lower == "scales" {
            let scales_val = Value::from_array(([3usize], vec![noise_scale, length_scale, noise_w]))
                .map_err(|e| anyhow!("Failed to create scales tensor: {}", e))?;
            inputs.insert(name.clone(), scales_val.into());
        } else if name_lower == "noise_scale" {
            let val = Value::from_array(([1usize], vec![noise_scale]))
                .map_err(|e| anyhow!("Failed to create noise_scale tensor: {}", e))?;
            inputs.insert(name.clone(), val.into());
        } else if name_lower == "length_scale" {
            let val = Value::from_array(([1usize], vec![length_scale]))
                .map_err(|e| anyhow!("Failed to create length_scale tensor: {}", e))?;
            inputs.insert(name.clone(), val.into());
        } else if name_lower == "noise_scale_w" || name_lower == "noise_w" {
            let val = Value::from_array(([1usize], vec![noise_w]))
                .map_err(|e| anyhow!("Failed to create noise_scale_w tensor: {}", e))?;
            inputs.insert(name.clone(), val.into());
        } else if name_lower == "input_lengths" || name_lower == "sequence_lengths" || (name_lower.contains("length") && !name_lower.contains("scale")) {
            let len_val = Value::from_array(([1usize], vec![seq_len as i64]))
                .map_err(|e| anyhow!("Failed to create input_lengths tensor: {}", e))?;
            inputs.insert(name.clone(), len_val.into());
        } else if name_lower == "sid" || name_lower.contains("speaker") {
            let sid = speaker_id.unwrap_or(0);
            let sid_val = Value::from_array(([1usize], vec![sid]))
                .map_err(|e| anyhow!("Failed to create speaker_id tensor: {}", e))?;
            inputs.insert(name.clone(), sid_val.into());
        } else {
            let ids_val = Value::from_array(([1usize, seq_len], padded_ids.clone()))
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

    if let Some(output_val) = outputs.values().next() {
        if let Ok((_shape, tensor)) = output_val.try_extract_tensor::<f32>() {
            let pcm = tensor.to_vec();
            if pcm.is_empty() {
                return Err(anyhow!("VITS model produced empty audio output"));
            }
            return Ok(pcm);
        }
    }

    Err(anyhow!("Failed to extract PCM waveform from VITS output tensor"))
}
