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

    let shape = vec![1usize, num_mels, num_frames]; 
    eprintln!("🎙️ [Neural Vocoder] Executing ONNX Graph.");
    eprintln!("🎙️ [Neural Vocoder] Input name: {}, mel_data length: {}, num_frames: {}, num_mels: {}", input_name, mel_data.len(), num_frames, num_mels);

    let mut tts_inputs: HashMap<String, Value> = HashMap::new();
    
    if let Ok(val) = Value::from_array((shape.clone(), mel_data.to_vec())) {
        tts_inputs.insert(input_name, val.into());
    } else if let Ok(val) = Value::from_array((vec![1usize, num_frames, num_mels], mel_data.to_vec())) {
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
