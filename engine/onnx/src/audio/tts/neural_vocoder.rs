use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Run the Neural Vocoder (HiFi-GAN, HiFT, etc.) ONNX graph
pub fn synthesize_mel_to_pcm(
    session: &mut Session,
    mel_data: &[f32],
    num_frames: usize,
) -> Result<Vec<f32>> {
    if mel_data.is_empty() || num_frames == 0 {
        return Ok(Vec::new());
    }

    let num_mels = mel_data.len() / num_frames;

    let inputs = session.inputs();
    if inputs.is_empty() {
        return Err(anyhow!("Vocoder ONNX graph has no inputs!"));
    }
    let input_name = inputs[0].name().to_string();

    // Query model input dimensions to find the expected number of mels
    let mut expected_mels = num_mels; // default to whatever we got
    if let ort::value::ValueType::Tensor { shape, .. } = inputs[0].dtype() {
        for &dim in shape.iter() {
            if dim == 80 || dim == 100 {
                expected_mels = dim as usize;
            }
        }
    }

    let mut final_mel_data = mel_data.to_vec();
    let mut final_num_mels = num_mels;

    if num_mels != expected_mels {
        eprintln!("🎙️ [Neural Vocoder] Channel mismatch: Got {}, Expected {}. Resampling...", num_mels, expected_mels);
        let mut resampled_mel = vec![0.0f32; expected_mels * num_frames];
        for t in 0..num_frames {
            for m in 0..expected_mels {
                let src_m_float = if expected_mels > 1 {
                    (m as f32) * ((num_mels - 1) as f32 / (expected_mels - 1) as f32)
                } else {
                    0.0f32
                };
                let idx_low = src_m_float.floor() as usize;
                let idx_high = src_m_float.ceil().min((num_mels - 1) as f32) as usize;
                let weight = src_m_float - idx_low as f32;
                
                // mel_data layout is [num_mels, num_frames] flat
                let val_low = mel_data[idx_low * num_frames + t];
                let val_high = mel_data[idx_high * num_frames + t];
                resampled_mel[m * num_frames + t] = val_low * (1.0 - weight) + val_high * weight;
            }
        }
        final_mel_data = resampled_mel;
        final_num_mels = expected_mels;
    }

    let shape = vec![1usize, final_num_mels, num_frames]; 
    eprintln!("🎙️ [Neural Vocoder] Executing ONNX Graph.");
    eprintln!("🎙️ [Neural Vocoder] Input name: {}, final mel_data length: {}, num_frames: {}, expected_mels: {}", input_name, final_mel_data.len(), num_frames, final_num_mels);

    let mut tts_inputs: HashMap<String, Value> = HashMap::new();
    
    if let Ok(val) = Value::from_array((shape.clone(), final_mel_data.to_vec())) {
        tts_inputs.insert(input_name, val.into());
    } else if let Ok(val) = Value::from_array((vec![1usize, num_frames, final_num_mels], final_mel_data.to_vec())) {
        tts_inputs.insert(inputs[0].name().to_string(), val.into());
    } else {
        return Err(anyhow!("Failed to construct Mel tensor for Vocoder"));
    }

    let output_tensors = session
        .run(tts_inputs)
        .map_err(|e| anyhow!("Vocoder ONNX graph execution failed: {}", e))?;

    if let Some(val) = output_tensors.values().next() {
        if let Ok((_shape_slice, wav_tensor)) = val.try_extract_tensor::<f32>() {
            let mut pcm = wav_tensor.to_vec();
            let mut max_abs = 0.0f32;
            for &s in &pcm {
                if s.abs() > max_abs {
                    max_abs = s.abs();
                }
            }
            eprintln!("🎙️ [Neural Vocoder] Output PCM Length: {}, max_abs: {}", pcm.len(), max_abs);
            if max_abs > 1.0 {
                eprintln!("🎙️ [Neural Vocoder] Normalizing amplitude by dividing by {}", max_abs);
                for s in &mut pcm {
                    *s = *s / max_abs;
                }
            }
            return Ok(pcm);
        }
    }

    Err(anyhow!("Failed to extract PCM waveform from Vocoder output"))
}
