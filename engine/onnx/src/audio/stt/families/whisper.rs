//! Whisper ONNX family adapter (`input_features` log-Mel spectrogram encoder + autoregressive decoder).

use super::{SttEncoderKind, SttFamilyProfile};
use crate::audio::stt::spectrogram::compute_log_mel_spectrogram;
use crate::engine::OnnxEngine;
use anyhow::{anyhow, bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use ort::session::Session;
use ort::value::Value;

/// Resolves and validates the Whisper processor and special-token contract from
/// `preprocessor_config.json`, `config.json`, and `tokenizer.json`.
pub fn resolve_whisper_profile(engine: &OnnxEngine) -> Result<SttFamilyProfile> {
    let dir = engine
        .model_dir
        .as_ref()
        .context("STT model directory missing")?;
    let processor: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("preprocessor_config.json"))?)?;
    let field = |name: &str| -> Result<usize> {
        processor
            .get(name)
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .with_context(|| format!("STT processor requires positive {name}"))
    };
    let base = crate::audio::config::AudioConfig::from_model_dir(&engine.model_dir);
    let n_mels = field("feature_size")?;
    let sample_rate = u32::try_from(field("sampling_rate")?)?;
    let hop_length = field("hop_length")?;
    let n_fft = field("n_fft")?;
    let max_samples = field("n_samples")?;
    let max_frames = field("nb_max_frames")?;
    if n_fft < 2
        || n_fft > max_samples
        || !n_fft.is_multiple_of(2)
        || n_mels > n_fft / 2 + 1
        || max_samples.checked_div(hop_length) != Some(max_frames)
    {
        bail!("Inconsistent STT processor dimensions");
    }
    let tokenizer = engine.tokenizer.as_ref().context("STT tokenizer missing")?;
    let token = |name: &str| -> Result<i64> {
        tokenizer
            .token_to_id(name)
            .map(i64::from)
            .with_context(|| format!("STT tokenizer missing {name}"))
    };
    let start_of_transcript = token("<|startoftranscript|>")?;
    let transcribe_token = token("<|transcribe|>")?;
    let translate_token = token("<|translate|>")?;
    let no_timestamps_token = token("<|notimestamps|>")?;
    let end_of_text_token = token("<|endoftext|>")?;
    let max_target_positions = base
        .max_target_positions
        .filter(|cap| *cap > 4)
        .ok_or_else(|| anyhow!("STT decoder position limit missing or invalid"))?;

    Ok(SttFamilyProfile {
        sample_rate,
        max_samples,
        encoder_kind: SttEncoderKind::MelSpectrogram {
            n_mels,
            hop_length,
            n_fft,
            max_frames,
        },
        max_target_positions,
        decoder_start_token: start_of_transcript,
        eos_token: end_of_text_token,
        transcribe_token: Some(transcribe_token),
        translate_token: Some(translate_token),
        no_timestamps_token: Some(no_timestamps_token),
        tokens_per_second_budget: 15.0,
        min_token_budget: 64,
    })
}

/// Validates memory bounds, computes the log-Mel spectrogram, and runs the Whisper encoder
/// (`input_features` -> `last_hidden_state`).
pub fn encode_whisper_features(
    encoder: &mut Session,
    pcm_samples: &[f32],
    profile: &SttFamilyProfile,
    limits: MediaLimits,
) -> Result<(Vec<usize>, Vec<f32>)> {
    let SttEncoderKind::MelSpectrogram {
        n_mels,
        hop_length,
        n_fft,
        max_frames,
    } = profile.encoder_kind
    else {
        bail!("Whisper encoder requires MelSpectrogram profile");
    };

    let mel_bytes = n_mels
        .checked_mul(max_frames)
        .and_then(|n| n.checked_mul(std::mem::size_of::<f32>()))
        .ok_or_else(|| anyhow!("STT mel buffer size overflow"))?;
    limits.check_decoded_bytes(mel_bytes as u64)?;
    let sample_workspace = profile
        .max_samples
        .checked_mul(2)
        .and_then(|n| n_fft.checked_mul(4).and_then(|fft| n.checked_add(fft)))
        .and_then(|n| n.checked_mul(std::mem::size_of::<f32>()))
        .and_then(|n| n.checked_add(mel_bytes))
        .ok_or_else(|| anyhow!("STT processor workspace size overflow"))?;
    limits.check_decoded_bytes(sample_workspace as u64)?;

    let audio_cfg = crate::audio::config::AudioConfig {
        n_mels,
        sample_rate: profile.sample_rate,
        hop_length,
        n_fft,
        max_frames,
        max_samples: profile.max_samples,
        max_target_positions: Some(profile.max_target_positions),
        start_of_transcript: profile.decoder_start_token,
        transcribe_token: profile.transcribe_token.unwrap_or(0),
        translate_token: profile.translate_token.unwrap_or(0),
        no_timestamps_token: profile.no_timestamps_token.unwrap_or(0),
        end_of_text_token: profile.eos_token,
    };

    let (mel_flat, _actual_frames) = compute_log_mel_spectrogram(pcm_samples, &audio_cfg);
    let shape_vec = vec![1usize, n_mels, max_frames];
    let mel = Value::from_array((shape_vec, mel_flat))?;
    let outputs = encoder.run(ort::inputs!["input_features" => mel])?;
    let output = outputs
        .values()
        .next()
        .ok_or_else(|| anyhow!("STT encoder output missing"))?;
    let (shape, data) = output.try_extract_tensor::<f32>()?;
    Ok((
        shape.iter().map(|&n| n as usize).collect::<Vec<_>>(),
        data.to_vec(),
    ))
}
