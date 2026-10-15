//! Shared channel/rate preparation. Backend callers explicitly choose normalization.

use crate::audio::check_pcm_workspace;
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedAudio, media_limits::MediaLimits};
use rayon::prelude::*;

/// Preserve amplitudes by default; legacy ONNX STT explicitly selects LimitToUnit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeakNormalization {
    Preserve,
    LimitToUnit,
}

/// Averages interleaved channels and optionally scales only peaks above one.
pub fn prepare_mono(
    audio: &DecodedAudio,
    policy: PeakNormalization,
    limits: MediaLimits,
) -> Result<DecodedAudio> {
    let frames = audio.samples().len() / usize::from(audio.channels());
    let peak = audio
        .samples()
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    if audio.channels() == 1 && (policy == PeakNormalization::Preserve || peak <= 1.0) {
        check_pcm_workspace(limits, audio.samples().len())?;
        return Ok(audio.clone());
    }
    check_transform_workspace(limits, audio.samples().len(), frames)?;
    let mut mono = Vec::new();
    mono.try_reserve_exact(frames)?;
    mono.extend(
        audio
            .samples()
            .chunks_exact(usize::from(audio.channels()))
            .map(|frame| {
                (frame.iter().map(|sample| f64::from(*sample)).sum::<f64>()
                    / f64::from(audio.channels())) as f32
            }),
    );
    if policy == PeakNormalization::LimitToUnit {
        let peak = mono
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        if peak > 1.0 {
            mono.iter_mut().for_each(|sample| *sample /= peak);
        }
    }
    Ok(DecodedAudio::new(audio.sample_rate(), 1, mono.into())?)
}

/// Existing linear interpolation, now reusable for any interleaved channel count.
/// This preserves the previous STT algorithm; it is not an anti-aliasing filter.
pub fn resample_linear(
    audio: &DecodedAudio,
    target_rate: u32,
    limits: MediaLimits,
) -> Result<DecodedAudio> {
    if target_rate == 0 {
        bail!("Target audio sample rate must be non-zero");
    }
    if audio.sample_rate() == target_rate {
        check_pcm_workspace(limits, audio.samples().len())?;
        return Ok(audio.clone());
    }
    let channels = usize::from(audio.channels());
    let input_frames = audio.samples().len() / channels;
    let output_frames = u64::try_from(input_frames)?
        .checked_mul(u64::from(target_rate))
        .context("Resampled frame count overflow")?
        / u64::from(audio.sample_rate());
    let output_len = usize::try_from(output_frames)?
        .checked_mul(channels)
        .context("Resampled sample count overflow")?;
    if output_len == 0 {
        bail!("Audio is too short for the requested sample rate");
    }
    check_transform_workspace(limits, audio.samples().len(), output_len)?;
    let mut output = Vec::new();
    output.try_reserve_exact(output_len)?;
    output.resize(output_len, 0.0);
    let ratio = f64::from(audio.sample_rate()) / f64::from(target_rate);
    output
        .par_chunks_exact_mut(channels)
        .enumerate()
        .for_each(|(frame, values)| {
            let position = frame as f64 * ratio;
            let index = (position as usize).min(input_frames - 1);
            let next = (index + 1).min(input_frames - 1);
            let fraction = position - index as f64;
            for (channel, value) in values.iter_mut().enumerate() {
                let first = f64::from(audio.samples()[index * channels + channel]);
                let second = f64::from(audio.samples()[next * channels + channel]);
                *value = (first + fraction * (second - first)) as f32;
            }
        });
    Ok(DecodedAudio::new(
        target_rate,
        audio.channels(),
        output.into(),
    )?)
}

fn check_transform_workspace(limits: MediaLimits, input: usize, output: usize) -> Result<()> {
    let samples = output
        .checked_mul(2)
        .and_then(|n| n.checked_add(input))
        .context("PCM transform workspace overflow")?;
    check_pcm_workspace(limits, samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_preserve_and_explicit_peak_policy() {
        let audio = DecodedAudio::new(8_000, 2, vec![2.0, 4.0, -4.0, -2.0].into()).unwrap();
        let kept =
            prepare_mono(&audio, PeakNormalization::Preserve, MediaLimits::default()).unwrap();
        assert_eq!(kept.samples(), &[3.0, -3.0]);
        let scaled = prepare_mono(
            &audio,
            PeakNormalization::LimitToUnit,
            MediaLimits::default(),
        )
        .unwrap();
        assert_eq!(scaled.samples(), &[1.0, -1.0]);
        assert_eq!(audio.samples(), &[2.0, 4.0, -4.0, -2.0]);
        assert!(prepare_mono(
            &audio,
            PeakNormalization::Preserve,
            MediaLimits::new(1, 1).unwrap()
        )
        .is_err());
    }

    #[test]
    fn test_resample_stereo_up_down_identity_and_invalid_target() {
        let audio = DecodedAudio::new(8_000, 2, vec![0.0, 1.0, 1.0, 0.0].into()).unwrap();
        let up = resample_linear(&audio, 16_000, MediaLimits::default()).unwrap();
        assert_eq!(up.samples(), &[0.0, 1.0, 0.5, 0.5, 1.0, 0.0, 1.0, 0.0]);
        assert_eq!((up.sample_rate(), up.channels()), (16_000, 2));
        let down = resample_linear(&up, 8_000, MediaLimits::default()).unwrap();
        assert_eq!(down.samples(), audio.samples());
        let same = resample_linear(&audio, 8_000, MediaLimits::default()).unwrap();
        assert_eq!(same.samples().as_ptr(), audio.samples().as_ptr());
        assert!(resample_linear(&audio, 0, MediaLimits::default()).is_err());
        assert!(resample_linear(&audio, u32::MAX, MediaLimits::new(1, 1).unwrap()).is_err());
        assert!(resample_linear(&audio, 1, MediaLimits::default()).is_err());
    }
}
