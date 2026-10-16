//! Backend-neutral decoded media. Codec and model-specific processing live in their own crates.

use crate::media_limits::MediaLimits;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use std::sync::Arc;

/// Default timeout for one remote media source, configurable in settings.
pub const DEFAULT_MEDIA_HTTP_TIMEOUT_MS: NonZeroU64 = match NonZeroU64::new(30_000) {
    Some(value) => value,
    None => NonZeroU64::MIN,
};

/// Configurable encoder-workspace preflight estimate, not a measured peak or OS reservation.
pub const DEFAULT_PROJECTOR_WORKSPACE_BYTES: NonZeroU64 = match NonZeroU64::new(crate::GIB) {
    Some(value) => value,
    None => NonZeroU64::MIN,
};

/// Shared media policy, independent of either inference backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MediaSettings {
    pub limits: MediaLimits,
    pub http_timeout_ms: NonZeroU64,
    pub projector_workspace_bytes: NonZeroU64,
    pub video: VideoSettings,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self {
            limits: MediaLimits::default(),
            http_timeout_ms: DEFAULT_MEDIA_HTTP_TIMEOUT_MS,
            projector_workspace_bytes: DEFAULT_PROJECTOR_WORKSPACE_BYTES,
            video: VideoSettings::default(),
        }
    }
}

/// Bounded frame sampling policy. These are limits, not an inference speed promise.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct VideoSettings {
    pub sample_interval_ms: NonZeroU64,
    pub max_frames: std::num::NonZeroU32,
    pub max_duration_ms: NonZeroU64,
    pub max_frame_pixels: NonZeroU64,
    pub deadline_ms: NonZeroU64,
    pub max_packets: NonZeroU64,
    pub max_decoded_frames: NonZeroU64,
}

impl Default for VideoSettings {
    fn default() -> Self {
        // Literal values are nonzero; MIN keeps construction total without panics.
        Self {
            sample_interval_ms: NonZeroU64::new(1_000).unwrap_or(NonZeroU64::MIN),
            max_frames: std::num::NonZeroU32::new(16).unwrap_or(std::num::NonZeroU32::MIN),
            max_duration_ms: NonZeroU64::new(30_000).unwrap_or(NonZeroU64::MIN),
            max_frame_pixels: NonZeroU64::new(4_194_304).unwrap_or(NonZeroU64::MIN),
            deadline_ms: NonZeroU64::new(30_000).unwrap_or(NonZeroU64::MIN),
            max_packets: NonZeroU64::new(100_000).unwrap_or(NonZeroU64::MIN),
            max_decoded_frames: NonZeroU64::new(10_000).unwrap_or(NonZeroU64::MIN),
        }
    }
}

/// Number of bytes in one packed RGB8 pixel.
pub const RGB8_CHANNELS: usize = 3;

/// Ordered incoming content. A source is a locator at transport boundaries and
/// can be an owned resolved-source handle inside the preparation layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputPart<Source = String> {
    Text {
        text: String,
    },
    Image {
        source: Source,
    },
    Audio {
        source: Source,
    },
    Document {
        source: Source,
    },
    Video {
        source: Source,
        #[serde(default)]
        include_audio: bool,
    },
}

/// Prepared content contains actual data, never file locators disguised as text.
/// Decoder/model buffers are kept in process rather than serialized into prompts.
#[derive(Debug, Clone)]
pub enum PreparedPart {
    Text { text: String },
    Image { image: DecodedImage },
    Audio { audio: DecodedAudio },
}

impl PreparedPart {
    /// Retained source buffers, including text capacity, without cloning their contents.
    pub fn request_bytes(messages: &[crate::ChatMessage<Vec<Self>>]) -> anyhow::Result<u64> {
        use anyhow::Context;
        messages
            .iter()
            .flat_map(|message| &message.content)
            .try_fold(0_u64, |sum, part| {
                sum.checked_add(u64::try_from(part.retained_bytes())?)
                    .context("Prepared request byte count overflow")
            })
    }

    /// Retained content bytes for request-level budgets (not a model token estimate).
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::Text { text } => text.capacity(),
            Self::Image { image } => image.pixels().len(),
            Self::Audio { audio } => std::mem::size_of_val(audio.samples()),
        }
    }

    /// Uses the existing backend modality contract rather than a parallel enum.
    pub fn modality(&self) -> crate::contracts::Modality {
        match self {
            Self::Text { .. } => crate::contracts::Modality::Text,
            Self::Image { .. } => crate::contracts::Modality::Image,
            Self::Audio { .. } => crate::contracts::Modality::Audio,
        }
    }
}

/// Invalid decoded data is rejected before a backend receives it.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MediaBufferError {
    #[error("Image dimensions must be non-zero and fit the buffer address space")]
    InvalidImageDimensions,
    #[error("RGB8 buffer requires {expected} bytes, received {actual}")]
    ImageLength { expected: usize, actual: usize },
    #[error("Audio requires a non-zero sample rate, channel count and whole, non-empty frames")]
    InvalidAudioLayout,
    #[error("Audio samples must be finite")]
    NonFiniteAudio,
}

/// Packed RGB8 pixels in row order, with no row padding. Clones share immutable storage.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl DecodedImage {
    /// Validates the decoded RGB layout; this does not decode an encoded image file.
    pub fn new(width: u32, height: u32, pixels: Arc<[u8]>) -> Result<Self, MediaBufferError> {
        let expected = usize::try_from(width)
            .ok()
            .zip(usize::try_from(height).ok())
            .filter(|&(w, h)| w > 0 && h > 0)
            .and_then(|(w, h)| w.checked_mul(h))
            .and_then(|area| area.checked_mul(RGB8_CHANNELS))
            .ok_or(MediaBufferError::InvalidImageDimensions)?;
        if pixels.len() != expected {
            return Err(MediaBufferError::ImageLength {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Borrow the RGB8 buffer without copying it.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Interleaved PCM F32 samples with an explicit rate and channel count.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    sample_rate: u32,
    channels: u16,
    samples: Arc<[f32]>,
}

impl DecodedAudio {
    /// Validates PCM layout without changing sample rate, channels or amplitude.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        samples: Arc<[f32]>,
    ) -> Result<Self, MediaBufferError> {
        if sample_rate == 0
            || channels == 0
            || samples.is_empty()
            || !samples.len().is_multiple_of(usize::from(channels))
        {
            return Err(MediaBufferError::InvalidAudioLayout);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(MediaBufferError::NonFiniteAudio);
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    /// Sample rate in Hz; it is not the PCM number format.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Borrow the PCM samples without copying them.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_old_settings_load_defaults_and_invalid_policy_rejects() {
        let settings: crate::ActiveUserSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.media, MediaSettings::default());
        assert!(serde_json::from_str::<MediaSettings>(r#"{"http_timeout_ms":0}"#).is_err());
        assert!(serde_json::from_str::<MediaSettings>(r#"{"limit_typo":1}"#).is_err());
        assert!(
            serde_json::from_str::<MediaSettings>(r#"{"limits":{"max_input_bytes":0}}"#).is_err()
        );
    }

    #[test]
    fn test_image_valid_rgb_keeps_dimensions_and_shares_pixels() {
        let image = DecodedImage::new(2, 1, Arc::from([255, 0, 0, 0, 255, 0])).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.pixels(), &[255, 0, 0, 0, 255, 0]);
        assert_eq!(image.pixels().as_ptr(), image.clone().pixels().as_ptr());
    }

    #[test]
    fn test_image_invalid_dimensions_and_byte_count_return_errors() {
        assert_eq!(
            DecodedImage::new(0, 1, Arc::from([])).unwrap_err(),
            MediaBufferError::InvalidImageDimensions
        );
        assert!(DecodedImage::new(u32::MAX, u32::MAX, Arc::from([])).is_err());
        assert_eq!(
            DecodedImage::new(1, 1, Arc::from([0, 0])).unwrap_err(),
            MediaBufferError::ImageLength {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn test_audio_valid_stereo_preserves_rate_and_shares_samples() {
        let audio = DecodedAudio::new(48_000, 2, Arc::from([0.25, -0.5, 0.0, 1.5])).unwrap();
        assert_eq!((audio.sample_rate(), audio.channels()), (48_000, 2));
        assert_eq!(audio.samples(), &[0.25, -0.5, 0.0, 1.5]);
        assert_eq!(audio.samples().as_ptr(), audio.clone().samples().as_ptr());
    }

    #[test]
    fn test_audio_invalid_rate_channels_and_frames_return_errors() {
        for (rate, channels, samples) in [
            (0, 1, vec![0.0]),
            (16_000, 0, vec![0.0]),
            (16_000, 1, vec![]),
            (16_000, 2, vec![0.0]),
        ] {
            assert_eq!(
                DecodedAudio::new(rate, channels, samples.into()).unwrap_err(),
                MediaBufferError::InvalidAudioLayout
            );
        }
    }

    #[test]
    fn test_audio_non_finite_samples_return_errors() {
        for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                DecodedAudio::new(16_000, 1, Arc::from([sample])).unwrap_err(),
                MediaBufferError::NonFiniteAudio
            );
        }
    }
}
