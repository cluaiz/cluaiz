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
    /// Explicit supported language codes (empty means dynamic tokenizer vocabulary lookup, as in Whisper).
    pub supported_languages: Vec<String>,
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

    /// Validates that the caller's requested language is supported by this STT model profile.
    pub fn validate_requested_language(&self, requested_lang: &str) -> Result<()> {
        if self.supported_languages.is_empty() {
            return Ok(());
        }
        let norm = requested_lang.trim().to_ascii_lowercase();
        if norm.is_empty() || norm == "auto" {
            return Ok(());
        }
        let primary = norm.split(['-', '_']).next().unwrap_or(norm.as_str());
        if self
            .supported_languages
            .iter()
            .any(|supported| supported == &norm || supported == primary)
        {
            return Ok(());
        }
        bail!(
            "Requested language '{}' is not supported by this STT model (supported: {}). Use a multilingual Whisper model for '{}' transcription.",
            requested_lang.trim(),
            self.supported_languages.join(", "),
            requested_lang.trim()
        )
    }
}

/// The language code of a language token (`<|hi|>`, `<|code_hi|>` → `hi`); None for other tokens.
pub fn language_of_token(token: &str) -> Option<&str> {
    let inner = token.strip_prefix("<|")?.strip_suffix("|>")?;
    let inner = inner.strip_prefix("code_").unwrap_or(inner);
    let is_code = (2..=5).contains(&inner.len()) && inner.chars().all(|c| c.is_ascii_lowercase());
    let is_task = ["transcribe", "translate", "notimestamps", "startof"]
        .iter()
        .any(|task| inner.contains(task));
    (is_code && !is_task).then_some(inner)
}

/// `hi-IN`, `HI`, `en_US` → `hi`, `hi`, `en` (STT language tokens carry the base language only).
pub fn base_language(code: &str) -> String {
    code.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The languages an STT model can be told to transcribe, from its tokenizer's language tokens
/// (Whisper: about 100). Empty for a model without them (Moonshine reads one language).
pub fn tokenizer_languages(model_dir: &std::path::Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(model_dir.join("tokenizer.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    let mut codes: Vec<String> = json
        .get("added_tokens")
        .and_then(|tokens| tokens.as_array())
        .into_iter()
        .flatten()
        .filter_map(|token| token.get("content")?.as_str())
        .filter_map(language_of_token)
        .map(str::to_owned)
        .collect();
    codes.sort();
    codes.dedup();
    codes
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tokens_are_told_apart_from_task_and_timestamp_tokens() {
        assert_eq!(language_of_token("<|hi|>"), Some("hi"));
        assert_eq!(language_of_token("<|haw|>"), Some("haw"));
        assert_eq!(language_of_token("<|code_en|>"), Some("en"));
        for other in ["<|transcribe|>", "<|translate|>", "<|notimestamps|>", "<|29.38|>", "hi", "<|EN|>"] {
            assert_eq!(language_of_token(other), None, "{other}");
        }
        assert_eq!(base_language(" hi-IN "), "hi");
        assert_eq!(base_language("EN_us"), "en");
    }

    #[test]
    fn tokenizer_languages_lists_the_language_tokens_once_each() {
        let dir = std::env::temp_dir().join(format!("cluaiz-stt-langs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("tokenizer.json"),
            r#"{"added_tokens":[{"content":"<|startoftranscript|>"},{"content":"<|hi|>"},{"content":"<|en|>"},{"content":"<|transcribe|>"},{"content":"<|0.00|>"},{"content":"<|en|>"}]}"#,
        )
        .unwrap();
        assert_eq!(tokenizer_languages(&dir), vec!["en", "hi"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(tokenizer_languages(&dir).is_empty());
    }
}
