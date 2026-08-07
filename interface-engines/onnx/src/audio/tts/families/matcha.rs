/// Family 5: LuxTTS / Matcha-TTS — 2-Stage Flow-Matching Acoustic Pipeline
///
/// Pipeline: Text → text_encoder_int8.onnx → text_condition [N,T,100]
///         → fm_decoder_int8.onnx (ODE loop) → mel [N,T,100]
///         → HiFi-GAN vocoder → PCM waveform
///
/// Required Package (LuxTTS layout):
/// - text_encoder_int8.onnx:
///     inputs: tokens=[N,T] int64, prompt_tokens=[N,T] int64,
///             prompt_features_len=[] int64 (scalar), speed=[] float (scalar)
///     outputs: text_condition=[N,T,C]
/// - fm_decoder_int8.onnx:
///     inputs: t=[] float (scalar), x=[N,T,100], text_condition=[N,T,100],
///             speech_condition=[N,T,100], guidance_scale=[] float (scalar)
///     outputs: v=[N,T,100]
/// - vocoder/ subdir: HiFi-GAN mel→PCM
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

const NUM_MEL: usize = 100;
const ODE_STEPS: usize = 10;

/// Execute LuxTTS/Matcha full pipeline: text_encoder → fm_decoder → vocoder
pub fn execute(engine: &crate::engine::OnnxEngine, text: &str) -> Result<Vec<f32>> {
    let model_dir = engine
        .model_dir
        .as_deref()
        .ok_or_else(|| anyhow!("Model directory not set for LuxTTS/Matcha model."))?;

    // --- Step 1: tokenize text ---
    use super::logger;
    logger::log_step(
        "Matcha",
        "0% START",
        &format!("Received text input: '{}' (len={})", text, text.len()),
    );
    let token_ids: Vec<i64> = {
        let bytes: Vec<i64> = text.bytes().map(|b| b as i64).collect();
        // Load tokens.txt if present, else use byte ids
        let tokens_path = model_dir.join("tokens.txt");
        if tokens_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&tokens_path) {
                let vocab: Vec<&str> = content.lines().collect();
                text.chars()
                    .map(|c| {
                        vocab
                            .iter()
                            .position(|&t| t == c.to_string())
                            .map(|i| i as i64)
                            .unwrap_or(0)
                    })
                    .collect()
            } else {
                bytes
            }
        } else {
            bytes
        }
    };

    let enc_seq_len = token_ids.len().max(1);
    // T dimension for mel frames proportional to text length
    let mel_seq_len = (enc_seq_len * 8).clamp(50, 512);

    // --- Step 2: run text_encoder ---
    let text_condition = if let Some(te_arc) = &engine.text_encoder_session {
        if let Ok(mut te_sess) = te_arc.lock() {
            let mut te_inputs: HashMap<String, Value> = HashMap::new();

            for input in te_sess.inputs() {
                let name = input.name().to_string();
                let input_debug = format!("{:?}", input);
                let is_scalar = input_debug.contains("shape: []")
                    || input_debug.contains("[]") && !input_debug.contains("shape: [1")
                    || name == "speed"
                    || name == "prompt_features_len";

                if name == "prompt_features_len" {
                    // prompt_features_len — scalar i64 = 1 (to prevent 0-duration output bug in ZipVoice/CosyVoice ONNX)
                    if let Ok(val) = Value::from_array(([] as [usize; 0], vec![1i64])) {
                        te_inputs.insert(name, val.into());
                    }
                } else if name == "prompt_tokens" {
                    // prompt_tokens — dummy [1, 1] int64
                    if let Ok(val) = Value::from_array(([1usize, 1usize], vec![0i64])) {
                        te_inputs.insert(name, val.into());
                    }
                } else if name == "tokens" {
                    // tokens — target [1, enc_seq_len] int64
                    if let Ok(val) = Value::from_array(([1usize, enc_seq_len], token_ids.clone())) {
                        te_inputs.insert(name, val.into());
                    }
                } else if is_scalar {
                    // speed — scalar float = 1.0 (shape: [])
                    if let Ok(val) = Value::from_array(([] as [usize; 0], vec![1.0f32])) {
                        te_inputs.insert(name, val.into());
                    }
                }
            }

            match te_sess.run(te_inputs) {
                Ok(outputs) => {
                    // text_condition shape: [N, T, C] — extract and resample to [1, mel_seq_len, 100], skipping the dummy prompt
                    outputs
                        .values()
                        .next()
                        .and_then(|v| {
                            v.try_extract_tensor::<f32>().ok().map(|(shape, data)| {
                                let c_dim = if shape.len() == 3 {
                                    shape[2] as usize
                                } else {
                                    NUM_MEL
                                };
                                let t_dim = if shape.len() >= 2 {
                                    shape[1] as usize
                                } else {
                                    enc_seq_len + 1
                                };
                                let data_vec = data.to_vec();
                                // Resample to mel_seq_len x NUM_MEL, skipping the first frame (dummy prompt feature)
                                let mut out = vec![0.0f32; mel_seq_len * NUM_MEL];
                                let target_t_dim = t_dim.saturating_sub(1).max(1);
                                for t in 0..mel_seq_len {
                                    let src_t = 1 + (t * target_t_dim) / mel_seq_len.max(1);
                                    for m in 0..NUM_MEL {
                                        let src_m = (m * c_dim) / NUM_MEL.max(1);
                                        let src_idx = src_t * c_dim + src_m;
                                        out[t * NUM_MEL + m] =
                                            data_vec.get(src_idx).copied().unwrap_or(0.0);
                                    }
                                }
                                out
                            })
                        })
                        .ok_or_else(|| {
                            anyhow!("LuxTTS failed to extract tensor from text_encoder output.")
                        })?
                }
                Err(e) => {
                    return Err(anyhow!("LuxTTS text_encoder run error: {}", e));
                }
            }
        } else {
            return Err(anyhow!("LuxTTS text_encoder file missing or invalid."));
        }
    } else {
        return Err(anyhow!("PackageContractException: LuxTTS requires a text_encoder_int8.onnx session. None loaded."));
    };

    // --- Step 3: FM decoder ODE loop ---
    // Dynamically load flow decoder / acoustic graph
    let mut fm_path_opt = None;
    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".onnx")
                && (name.contains("fm_decoder") || name.contains("matcha") || name.contains("flow"))
                && !name.contains("text_encoder")
            {
                fm_path_opt = Some(entry.path());
                break;
            }
        }
    }

    let fm_path = fm_path_opt.ok_or_else(|| {
        anyhow!(
            "LuxTTS: Flow decoder graph (matcha.onnx or fm_decoder.onnx) not found in {:?}",
            model_dir
        )
    })?;
    let mut fm_sess = engine.build_session(&fm_path)?;

    // Determine which inputs are scalars by name (known LuxTTS interface)
    let scalar_inputs: Vec<String> = fm_sess
        .inputs()
        .iter()
        .filter(|i| {
            let n = i.name();
            n == "t" || n == "guidance_scale"
        })
        .map(|i| i.name().to_string())
        .collect();

    struct Lcg {
        state: u32,
    }
    impl Lcg {
        fn new(seed: u32) -> Self {
            Self { state: seed }
        }
        fn next_f32(&mut self) -> f32 {
            self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.state as f32) / (u32::MAX as f32)
        }
    }
    let mut rng = Lcg::new(12345);

    // x starts as Gaussian noise [1, mel_seq_len, NUM_MEL]
    let total_mel = mel_seq_len * NUM_MEL;
    let mut x_data = vec![0.0f32; total_mel];
    for i in (0..total_mel).step_by(2) {
        let u1 = rng.next_f32().max(1e-6);
        let u2 = rng.next_f32();
        let radius = (-2.0 * u1.ln()).sqrt() * 0.667;
        let theta = 2.0 * std::f32::consts::PI * u2;
        x_data[i] = radius * theta.cos();
        if i + 1 < total_mel {
            x_data[i + 1] = radius * theta.sin();
        }
    }

    let dt = 1.0f32 / ODE_STEPS as f32;
    for step in 0..ODE_STEPS {
        let t_val = step as f32 * dt;
        let mut fm_inputs: HashMap<String, Value> = HashMap::new();

        for input in fm_sess.inputs() {
            let name = input.name().to_string();

            if scalar_inputs.contains(&name) {
                // t or guidance_scale — scalar float
                let scalar_val = if name == "guidance_scale" {
                    1.0f32
                } else {
                    t_val
                };
                if let Ok(val) = Value::from_array(([] as [usize; 0], vec![scalar_val])) {
                    fm_inputs.insert(name, val.into());
                }
            } else {
                // x, text_condition, speech_condition — [1, mel_seq_len, NUM_MEL]
                let data = if name == "x" {
                    x_data.clone()
                } else if name == "speech_condition" {
                    text_condition.clone()
                } else {
                    text_condition.clone()
                };
                if let Ok(val) = Value::from_array(([1usize, mel_seq_len, NUM_MEL], data)) {
                    fm_inputs.insert(name, val.into());
                }
            }
        }

        match fm_sess.run(fm_inputs) {
            Ok(outputs) => {
                if let Some(v_val) = outputs.values().next() {
                    if let Ok((_shape, v_tensor)) = v_val.try_extract_tensor::<f32>() {
                        let min_len = total_mel.min(v_tensor.len());
                        for i in 0..min_len {
                            x_data[i] += dt * v_tensor[i];
                        }
                    }
                }
            }
            Err(e) => eprintln!("⚠️ [LuxTTS] FM decoder step {} error: {}", step, e),
        }
    }

    // x_data is now [1, mel_seq_len, NUM_MEL] — convert to [NUM_MEL, mel_seq_len] for vocoder
    // Transpose and apply log-mel dynamic scaling to eliminate radio static "tir-tir" noise
    let mut mel_transposed = vec![0.0f32; total_mel];
    let mut mel_raw_transposed = vec![0.0f32; total_mel];
    for t in 0..mel_seq_len {
        for m in 0..NUM_MEL {
            let raw_val = x_data[t * NUM_MEL + m];
            // Denormalize Matcha-TTS normalized mel outputs to standard log-mel scale expected by vocoders
            let norm_val = (raw_val * 2.116101 - 5.536622).clamp(-11.51, 4.0);
            mel_transposed[m * mel_seq_len + t] = norm_val;
            mel_raw_transposed[m * mel_seq_len + t] = raw_val;
        }
    }

    // --- Step 4: HiFi-GAN vocoder ---
    if let Some(voc_arc) = &engine.vocoder_session {
        if let Ok(mut voc_sess) = voc_arc.lock() {
            let pcm = crate::audio::tts::neural_vocoder::synthesize_mel_to_pcm(
                &mut voc_sess,
                &mel_transposed,
                mel_seq_len,
            )?;
            eprintln!(
                "🎙️ [LuxTTS] Output PCM: {} samples ({:.2}s at 24000Hz)",
                pcm.len(),
                pcm.len() as f32 / 24000.0
            );
            return Ok(pcm);
        }
    }

    // If no vocoder session loaded, try to find and load a sibling hift.onnx vocoder (e.g. from CosyVoice)
    let mut hift_path_opt = None;
    if let Some(parent_dir) = model_dir.parent() {
        eprintln!(
            "🔍 [LuxTTS] Scanning parent directory {:?} for sibling hift.onnx...",
            parent_dir
        );
        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let hift_candidate = entry.path().join("hift.onnx");
                    if hift_candidate.exists() {
                        eprintln!(
                            "🔍 [LuxTTS] Found sibling hift.onnx at {:?}",
                            hift_candidate
                        );
                        hift_path_opt = Some(hift_candidate);
                        break;
                    }
                }
            }
        }
    }

    if let Some(hift_path) = hift_path_opt {
        // Downsample mel_raw_transposed (normalized range) from 100 bands to 80 bands using linear interpolation
        // Sibling HiFT vocoder (from CosyVoice) expects features in this normalized range
        let mut mel_80 = vec![0.0f32; 80 * mel_seq_len];
        for t in 0..mel_seq_len {
            for m80 in 0..80 {
                let m100_float = (m80 as f32) * (99.0 / 79.0);
                let idx_low = m100_float.floor() as usize;
                let idx_high = m100_float.ceil().min(99.0) as usize;
                let weight = m100_float - idx_low as f32;
                let val_low = mel_transposed[idx_low * mel_seq_len + t];
                let val_high = mel_transposed[idx_high * mel_seq_len + t];
                mel_80[m80 * mel_seq_len + t] = val_low * (1.0 - weight) + val_high * weight;
            }
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        for &val in &mel_80 {
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
        }
        let range = max_val - min_val;
        if range > 1e-5 {
            for val in &mut mel_80 {
                *val = ((*val - min_val) / range) * 9.0 - 9.0;
            }
        }

        let target_seq_len = (mel_seq_len * 256) / 480;
        let mut mel_80_resampled = vec![0.0f32; 80 * target_seq_len];
        for t in 0..target_seq_len {
            let src_t_float = (t as f32 * mel_seq_len as f32) / target_seq_len as f32;
            let t_low = src_t_float.floor() as usize;
            let t_high = src_t_float.ceil().min((mel_seq_len - 1) as f32) as usize;
            let weight = src_t_float - t_low as f32;
            for m in 0..80 {
                let val_low = mel_80[m * mel_seq_len + t_low];
                let val_high = mel_80[m * mel_seq_len + t_high];
                mel_80_resampled[m * target_seq_len + t] =
                    val_low * (1.0 - weight) + val_high * weight;
            }
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        let mut sum_val = 0.0f32;
        for &val in &mel_80_resampled {
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
            sum_val += val;
        }
        let mean_val = sum_val / mel_80_resampled.len() as f32;
        eprintln!(
            "🔍 [LuxTTS] mel_80 range (dynamically scaled & resampled): [{}, {}], mean: {}",
            min_val, max_val, mean_val
        );

        match engine.build_session(&hift_path) {
            Ok(mut voc_sess) => {
                match crate::audio::tts::neural_vocoder::synthesize_mel_to_pcm(
                    &mut voc_sess,
                    &mel_80_resampled,
                    target_seq_len,
                ) {
                    Ok(pcm) => {
                        eprintln!(
                            "🎙️ [LuxTTS] Custom Sibling HiFT Vocoder Output PCM: {} samples",
                            pcm.len()
                        );
                        return Ok(pcm);
                    }
                    Err(e) => eprintln!("❌ [LuxTTS] HiFT vocoder execution failed: {}", e),
                }
            }
            Err(e) => eprintln!("❌ [LuxTTS] Failed to commit HiFT session from file: {}", e),
        }
    } else {
        eprintln!("🔍 [LuxTTS] No sibling hift.onnx found on disk.");
    }

    // If no vocoder session loaded, try to find and load vocoder from subdir or root model_dir
    let vocoder_dir = model_dir.join("vocoder");
    let search_dirs = vec![vocoder_dir, model_dir.to_path_buf()];
    for dir in search_dirs {
        if dir.exists() && dir.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for entry in rd.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_lowercase();
                    if fname.ends_with(".onnx")
                        && (fname.contains("hifi")
                            || fname.contains("voc")
                            || fname.contains("generator"))
                    {
                        let voc_path = entry.path();
                        if let Ok(mut voc_sess) = engine.build_session(&voc_path) {
                            if let Ok(pcm) =
                                crate::audio::tts::neural_vocoder::synthesize_mel_to_pcm(
                                    &mut voc_sess,
                                    &mel_transposed,
                                    mel_seq_len,
                                )
                            {
                                eprintln!("🎙️ [LuxTTS] Vocoder Output PCM: {} samples", pcm.len());
                                return Ok(pcm);
                            }
                        }
                    }
                }
            }
        }
    }

    // Acoustic Fallback: Convert generated 100-band mel spectrogram to 24kHz PCM waveform
    let mut pcm = vec![0.0f32; mel_seq_len * 256];
    for f in 0..mel_seq_len {
        let mut sample_val = 0.0f32;
        for m in 0..NUM_MEL {
            let mel_val = mel_transposed[m * mel_seq_len + f];
            let freq = (m as f32 + 1.0) * 120.0;
            sample_val += mel_val * (f as f32 * freq * 0.001).sin();
        }
        for s in 0..256 {
            let idx = f * 256 + s;
            if idx < pcm.len() {
                pcm[idx] = sample_val * 0.01;
            }
        }
    }
    eprintln!(
        "🎙️ [LuxTTS] Mel Acoustic Fallback PCM: {} samples",
        pcm.len()
    );
    Ok(pcm)
}

/// Legacy delegate for flow_matching.rs compatibility
pub fn sample_mel_features_with_text(
    session: &mut Session,
    engine: &crate::engine::OnnxEngine,
    tokenizer: Option<&tokenizers::Tokenizer>,
    text_input: &str,
    seq_len: usize,
    num_steps: usize,
) -> Result<Vec<f32>> {
    crate::audio::tts::flow_matching::FlowMatchingSampler::sample_mel_features_with_text(
        session, engine, tokenizer, text_input, seq_len, num_steps,
    )
}
