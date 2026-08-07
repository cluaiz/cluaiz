/// Family 1: Kokoro-82M — Style-Conditioned Phoneme & Style Vector Synthesizer
///
/// Pipeline: Text → Phoneme Tokenizer → [Tokens + Style Vector] → Kokoro-82M.onnx → PCM Float32
///
/// Assets:
/// - model.onnx / model_uint8.onnx
/// - tokenizer.json / config.json
/// - voices/*.bin (510 style vectors x 256 dims)

use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;
use std::path::Path;

/// Load a named voice style vector from the model directory.
pub fn load_style_vector(model_dir: &Path, voice_name: &str) -> Result<Vec<f32>> {
    let voices_dir = model_dir.join("voices");
    let voice_file = voices_dir.join(format!("{}.bin", voice_name));

    if !voice_file.exists() {
        return Err(anyhow!(
            "Voice file not found: {:?}. Available voices should be in {:?}",
            voice_file, voices_dir
        ));
    }

    load_style_from_file(&voice_file)
}

/// Load the first available voice file from voices/ directory.
pub fn load_first_available_voice(model_dir: &Path) -> Result<Vec<f32>> {
    let voices_dir = model_dir.join("voices");

    if !voices_dir.exists() || !voices_dir.is_dir() {
        return Err(anyhow!("No voices/ directory found in model directory {:?}", model_dir));
    }

    let entries = std::fs::read_dir(&voices_dir)
        .map_err(|e| anyhow!("Cannot read voices/ directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "bin").unwrap_or(false) {
            eprintln!(
                "📖 [Kokoro Handler] Using first available voice: {:?}",
                path.file_name().unwrap_or_default()
            );
            return load_style_from_file(&path);
        }
    }

    Err(anyhow!("No .bin voice files found in {:?}", voices_dir))
}

fn load_style_from_file(path: &Path) -> Result<Vec<f32>> {
    let raw_bytes = std::fs::read(path)
        .map_err(|e| anyhow!("Failed to read voice file {:?}: {}", path, e))?;

    if raw_bytes.len() < 4 || raw_bytes.len() % 4 != 0 {
        return Err(anyhow!(
            "Voice file {:?} has invalid size ({} bytes, not a multiple of 4). Expected raw float32 data.",
            path, raw_bytes.len()
        ));
    }

    let style_dim = raw_bytes.len() / 4;
    let mut style_vector = Vec::with_capacity(style_dim);

    for chunk in raw_bytes.chunks_exact(4) {
        let bytes: [u8; 4] = chunk.try_into().unwrap();
        style_vector.push(f32::from_le_bytes(bytes));
    }

    for (i, &val) in style_vector.iter().enumerate() {
        if val.is_nan() || val.is_infinite() {
            return Err(anyhow!(
                "Voice file {:?} contains NaN/Inf at index {}. File may be corrupt.",
                path, i
            ));
        }
    }

    eprintln!(
        "📖 [Kokoro Handler] Loaded style vector: {:?} ({} dims)",
        path.file_name().unwrap_or_default(),
        style_dim
    );

    Ok(style_vector)
}

/// Execute Kokoro ONNX model with style-conditioned input.
pub fn execute(
    session: &mut Session,
    phoneme_ids: &[i64],
    style_vector: &[f32],
    speed: f32,
) -> Result<Vec<f32>> {
    if phoneme_ids.is_empty() {
        return Err(anyhow!("Cannot synthesize: empty phoneme ID sequence"));
    }
    if style_vector.is_empty() {
        return Err(anyhow!("Cannot synthesize: empty style vector"));
    }

    let seq_len = phoneme_ids.len();
    let style_dim = style_vector.len();
    let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();

    eprintln!(
        "🎙️ [Kokoro Handler] Executing with {} phoneme IDs, style_dim={}, speed={}",
        seq_len, style_dim, speed
    );

    let mut inputs: HashMap<String, Value> = HashMap::new();

    for name in &input_names {
        let name_lower = name.to_lowercase();

        if name_lower.contains("style") || name_lower.contains("voice") || name_lower.contains("embed") {
            let target_dim = 256;
            let max_tokens = 510;
            let len = if seq_len >= 2 { seq_len - 2 } else { seq_len };
            if len >= max_tokens {
                return Err(anyhow!(
                    "Kokoro sequence length ({}) exceeds max token style limit ({})",
                    len, max_tokens
                ));
            }
            let start = len * target_dim;
            let vec_slice = if style_vector.len() >= start + target_dim {
                &style_vector[start..start + target_dim]
            } else if style_vector.len() >= target_dim {
                let last = (style_vector.len() / target_dim - 1) * target_dim;
                &style_vector[last..last + target_dim]
            } else {
                style_vector
            };
            let style_val = Value::from_array(([1usize, vec_slice.len()], vec_slice.to_vec()))
                .map_err(|e| anyhow!("Failed to create style tensor: {}", e))?;
            inputs.insert(name.clone(), style_val.into());

        } else if name_lower.contains("speed") || name_lower.contains("rate") {
            let speed_val = Value::from_array(([1usize], vec![speed]))
                .map_err(|e| anyhow!("Failed to create speed tensor: {}", e))?;
            inputs.insert(name.clone(), speed_val.into());
        } else {
            let ids_val = Value::from_array(([1usize, seq_len], phoneme_ids.to_vec()))
                .map_err(|e| anyhow!("Failed to create input_ids tensor: {}", e))?;
            inputs.insert(name.clone(), ids_val.into());
        }
    }

    let outputs = session
        .run(inputs)
        .map_err(|e| anyhow!("Kokoro ONNX session execution failed: {}", e))?;

    if let Some(output_val) = outputs.values().next() {
        if let Ok((_shape, tensor)) = output_val.try_extract_tensor::<f32>() {
            let pcm = tensor.to_vec();
            if pcm.is_empty() {
                return Err(anyhow!("Kokoro model produced empty audio output"));
            }
            return Ok(pcm);
        }
    }

    Err(anyhow!("Failed to extract PCM waveform from Kokoro output tensor"))
}
