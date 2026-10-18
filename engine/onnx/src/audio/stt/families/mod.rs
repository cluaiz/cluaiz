//! Modular Speech-to-Text (STT) family adapters.
//!
//! Dispatches audio preprocessing, encoder execution, and decoder prompt initialization based on
//! the ONNX encoder graph's declared input tensor signature (`input_features` for Mel-spectrogram
//! encoders such as Whisper; `input_values` for raw-waveform encoders such as Moonshine) and the
//! model package's configuration files (`preprocessor_config.json`, `config.json`, `tokenizer.json`).

pub mod moonshine;
pub mod whisper;

use crate::engine::OnnxEngine;
use anyhow::{bail, Result};
use ort::session::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttEncoderKind {
    /// Fixed-window log-Mel spectrogram encoder (`input_features: float32[1, n_mels, max_frames]`).
    MelSpectrogram {
        n_mels: usize,
        hop_length: usize,
        n_fft: usize,
        max_frames: usize,
    },
    /// Variable-length raw PCM waveform encoder (`input_values: float32[1, num_samples]`).
    RawWaveform { do_normalize: bool },
}

#[derive(Debug, Clone)]
pub struct SttFamilyProfile {
    pub sample_rate: u32,
    pub max_samples: usize,
    pub encoder_kind: SttEncoderKind,
    pub max_target_positions: usize,
    pub decoder_start_token: i64,
    pub eos_token: i64,
    pub transcribe_token: Option<i64>,
    pub translate_token: Option<i64>,
    pub no_timestamps_token: Option<i64>,
    /// Token generation rate cap per second of audio (e.g. 6.5 for Moonshine, 15.0 for Whisper).
    pub tokens_per_second_budget: f32,
    /// Minimum token budget floor before clamping to `max_target_positions`.
    pub min_token_budget: usize,
}

impl SttFamilyProfile {
    /// Computes the maximum number of autoregressive decoder steps for a given audio duration.
    pub fn max_decode_steps(&self, audio_duration_secs: f32) -> usize {
        let reserved_prefix = if self.transcribe_token.is_some() { 4 } else { 2 };
        let hard_cap = self.max_target_positions.saturating_sub(reserved_prefix).max(1);
        let dynamic_budget = ((audio_duration_secs * self.tokens_per_second_budget)
            + (self.min_token_budget as f32))
            .ceil() as usize;
        dynamic_budget.clamp(1, hard_cap)
    }
}

/// Inspects the encoder session's input tensor name to determine whether it is a Mel-spectrogram
/// STT encoder (`input_features`) or a raw-waveform STT encoder (`input_values`).
pub fn encoder_input_name(encoder: &Session) -> Option<&str> {
    if encoder.inputs().len() != 1 {
        return None;
    }
    match encoder.inputs()[0].name() {
        "input_features" => Some("input_features"),
        "input_values" => Some("input_values"),
        _ => None,
    }
}

/// Resolves the `SttFamilyProfile` for the loaded model package based on its encoder input tensor.
pub fn resolve_stt_profile(
    engine: &OnnxEngine,
    encoder_input: &str,
) -> Result<SttFamilyProfile> {
    match encoder_input {
        "input_features" => whisper::resolve_whisper_profile(engine),
        "input_values" => moonshine::resolve_moonshine_profile(engine),
        other => bail!("Unsupported STT encoder input tensor: {other}"),
    }
}
