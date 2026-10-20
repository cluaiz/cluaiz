//! Checked mono PCM16 encoding shared by speech backends.
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use std::io::Cursor;

pub fn encode_pcm16(samples: &[f32], sample_rate: u32, limits: MediaLimits) -> Result<Vec<u8>> {
    if sample_rate == 0 || samples.iter().any(|sample| !sample.is_finite()) {
        bail!("WAV requires a positive sample rate and finite PCM samples");
    }
    let data_bytes = samples
        .len()
        .checked_mul(2)
        .context("WAV sample count overflow")?;
    let wav_bytes = data_bytes.checked_add(44).context("WAV length overflow")?;
    if data_bytes > (u32::MAX - 36) as usize {
        bail!("WAV exceeds the RIFF size limit");
    }
    // The caller's F32 input remains live beside this encoded output.
    let retained = samples
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(wav_bytes))
        .context("WAV buffer budget overflow")?;
    limits.check_decoded_bytes(u64::try_from(retained)?)?;
    let mut output = Cursor::new(Vec::with_capacity(wav_bytes));
    let mut writer = hound::WavWriter::new(
        &mut output,
        hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for &sample in samples {
        writer.write_sample((sample * 32767.0).clamp(-32768.0, 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(output.into_inner())
}

/// JSON/SSE speech transport, including simultaneously retained PCM/WAV/Base64 buffers.
pub fn encode_data_uri(samples: &[f32], sample_rate: u32, limits: MediaLimits) -> Result<String> {
    use base64::Engine;
    const PREFIX: &str = "data:audio/wav;base64,";
    let wav = encode_pcm16(samples, sample_rate, limits)?;
    let encoded = wav
        .len()
        .checked_add(2)
        .and_then(|n| (n / 3).checked_mul(4))
        .and_then(|n| n.checked_add(PREFIX.len()))
        .context("WAV data URI size overflow")?;
    let retained = samples
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(wav.len()))
        .and_then(|n| n.checked_add(encoded))
        .context("WAV data URI budget overflow")?;
    limits.check_decoded_bytes(u64::try_from(retained)?)?;
    let mut uri = String::with_capacity(encoded);
    uri.push_str(PREFIX);
    base64::engine::general_purpose::STANDARD.encode_string(wav, &mut uri);
    Ok(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_wav_roundtrip_clipping_and_rejections() -> Result<()> {
        let bytes = encode_pcm16(&[-2.0, 0.0, 2.0], 24_000, MediaLimits::default())?;
        let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
        assert_eq!(reader.spec().sample_rate, 24_000);
        assert_eq!(
            reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()?,
            [-32768, 0, 32767]
        );
        assert!(encode_pcm16(&[f32::NAN], 24_000, MediaLimits::default()).is_err());
        assert!(encode_pcm16(&[0.0], 0, MediaLimits::default()).is_err());
        assert!(encode_pcm16(&[0.0; 3], 24_000, MediaLimits::new(64, 61)?).is_err());
        assert!(encode_data_uri(&[0.0; 3], 24_000, MediaLimits::new(64, 100)?).is_err());
        Ok(())
    }
}
