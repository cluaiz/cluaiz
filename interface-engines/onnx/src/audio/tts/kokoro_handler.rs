use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;
use std::path::Path;

/// Dedicated Kokoro-82M TTS Execution Handler
///
/// [FACT] Kokoro ONNX input contract (verified from research):
///   - `input_ids`: int64 [1, N] — phoneme token sequence
///   - `style`: float32 [1, STYLE_DIM] — voice style embedding (typically 256-dim)
///   - `speed`: float32 [1] — speaking rate (1.0 = default)
///
/// [FACT] Style vectors are stored as raw float32 binary dumps in voices/*.bin
///   - Each .bin file = N * 4 bytes, where N is the style dimension
///   - Most Kokoro models use 256-dim style vectors

/// Load a named voice style vector from the model directory.
/// Searches for `voices/{voice_name}.bin` in the model directory.
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
/// Used as a fallback when no specific voice name matches.
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

    // Validate: check for NaN/Inf in style vector
    for (i, &val) in style_vector.iter().enumerate() {
        if val.is_nan() || val.is_infinite() {
            return Err(anyhow!(
                "Voice file {:?} contains NaN/Inf at index {}. File may be corrupt.",
                path, i
            ));
        }
    }

    eprintln!(
        "📖 [Kokoro Handler] Loaded style vector: {:?} ({} dims, range [{:.3}, {:.3}])",
        path.file_name().unwrap_or_default(),
        style_dim,
        style_vector.iter().cloned().fold(f32::INFINITY, f32::min),
        style_vector.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
    );

    Ok(style_vector)
}

/// Execute a Kokoro ONNX model with style-conditioned input.
///
/// # Arguments
/// * `session` - Loaded ONNX inference session for Kokoro model
/// * `phoneme_ids` - Sequence of phoneme/character IDs
/// * `style_vector` - Voice style embedding from voices/*.bin (typically 256-dim)
/// * `speed` - Speaking rate (1.0 = default, <1.0 = slower, >1.0 = faster)
pub fn execute_kokoro(
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
        "🎙️ [Kokoro Handler] Executing with {} phoneme IDs, style_dim={}, speed={}, inputs={:?}",
        seq_len, style_dim, speed, input_names
    );

    let mut inputs: HashMap<String, Value> = HashMap::new();

    for name in &input_names {
        let name_lower = name.to_lowercase();

        if name_lower.contains("style") || name_lower.contains("voice") || name_lower.contains("embed") {
            // Kokoro expects a [1, 256] style embedding vector.
            // Voice .bin files may contain multiple vectors (e.g. 510 * 256 = 130,560 floats).
            // Extract the first 256-dim vector if style_vector is larger.
            let target_dim = 256;
            let vec_slice = if style_vector.len() >= target_dim {
                let num_vectors = style_vector.len() / target_dim;
                // Voice .bin contains pre-computed style vectors for each seq_len.
                // We must select the slice matching our seq_len (or the last one if too long)
                let index = if seq_len < num_vectors { seq_len } else { num_vectors - 1 };
                let start = index * target_dim;
                let end = start + target_dim;
                &style_vector[start..end]
            } else {
                style_vector
            };
            let style_val = Value::from_array(([1usize, vec_slice.len()], vec_slice.to_vec()))
                .map_err(|e| anyhow!("Failed to create style tensor: {}", e))?;
            inputs.insert(name.clone(), style_val.into());
        } else if name_lower.contains("speed") || name_lower.contains("rate") {
            // Speed tensor: [1]
            let speed_val = Value::from_array(([1usize], vec![speed]))
                .map_err(|e| anyhow!("Failed to create speed tensor: {}", e))?;
            inputs.insert(name.clone(), speed_val.into());
        } else {
            // Primary input: phoneme IDs tensor [1, seq_len]
            let ids_val = Value::from_array(([1usize, seq_len], phoneme_ids.to_vec()))
                .map_err(|e| anyhow!("Failed to create input_ids tensor: {}", e))?;
            inputs.insert(name.clone(), ids_val.into());
        }
    }

    if inputs.is_empty() {
        return Err(anyhow!("Kokoro model has no recognized inputs. Expected: input_ids, style, speed"));
    }

    let outputs = session
        .run(inputs)
        .map_err(|e| anyhow!("Kokoro ONNX session execution failed: {}", e))?;

    // Extract PCM waveform from first output tensor
    if let Some(output_val) = outputs.values().next() {
        if let Ok((_shape, tensor)) = output_val.try_extract_tensor::<f32>() {
            let pcm = tensor.to_vec();
            eprintln!(
                "🎙️ [Kokoro Handler] Output PCM: {} samples ({:.2}s at 24000Hz)",
                pcm.len(),
                pcm.len() as f32 / 24000.0
            );
            if pcm.is_empty() {
                return Err(anyhow!("Kokoro model produced empty audio output"));
            }
            return Ok(pcm);
        }
    }

    Err(anyhow!("Failed to extract PCM waveform from Kokoro output tensor"))
}
