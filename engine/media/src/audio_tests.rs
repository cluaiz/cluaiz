use super::*;
use crate::{
    resample::{prepare_mono, resample_linear, PeakNormalization},
    source::read_local_or_inline,
};
use base64::Engine;

fn float_wav(channels: u16, rate: u32, samples: &[f32]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for &sample in samples {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    bytes.into_inner()
}

#[test]
fn test_audio_wav_preserves_rate_channels_samples_and_amplitude() {
    let samples = [2.0, -2.0, 0.25, 0.75];
    let decoded = decode_audio(float_wav(2, 8_000, &samples), MediaLimits::default()).unwrap();
    assert_eq!((decoded.sample_rate(), decoded.channels()), (8_000, 2));
    assert_eq!(decoded.samples(), &samples);
    assert_eq!(
        decoded.samples().as_ptr(),
        decoded.clone().samples().as_ptr()
    );
    let mut bytes = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for sample in [0_i16, 16_384, -16_384, i16::MIN] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let decoded = decode_audio(bytes.into_inner(), MediaLimits::default()).unwrap();
    assert_eq!(decoded.samples(), &[0.0, 0.5, -0.5, -1.0]);
}

#[test]
fn test_audio_source_file_uri_and_inline_feed_same_decoder_and_stt_policy() {
    let bytes = float_wav(2, 8_000, &[2.0, 4.0, -4.0, -2.0]);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("input audio.wav");
    std::fs::write(&file, &bytes).unwrap();
    let uri = reqwest::Url::from_file_path(&file).unwrap();
    let inline = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    for input in [file.to_str().unwrap(), uri.as_str(), inline.as_str()] {
        let loaded = read_local_or_inline(input, MediaLimits::default(), |_| Ok(())).unwrap();
        let decoded = decode_audio(loaded, MediaLimits::default()).unwrap();
        let mono = prepare_mono(
            &decoded,
            PeakNormalization::LimitToUnit,
            MediaLimits::default(),
        )
        .unwrap();
        let resampled = resample_linear(&mono, 16_000, MediaLimits::default()).unwrap();
        assert_eq!(resampled.samples(), &[1.0, 0.0, -1.0, -1.0]);
    }
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert!(read_local_or_inline(
        file.to_str().unwrap(),
        MediaLimits::default(),
        |_| anyhow::bail!("Denied")
    )
    .is_err());
    assert!(read_local_or_inline(&inline, MediaLimits::new(1, 1).unwrap(), |_| Ok(())).is_err());
    assert!(
        read_local_or_inline("https://invalid/audio", MediaLimits::default(), |_| Ok(())).is_err()
    );
}

#[test]
fn test_audio_empty_corrupt_nonfinite_and_limits_reject() {
    for bytes in [
        Vec::new(),
        b"corrupt audio".to_vec(),
        float_wav(1, 8_000, &[]),
        float_wav(1, 8_000, &[f32::NAN]),
    ] {
        assert!(decode_audio(bytes, MediaLimits::default()).is_err());
    }
    let bytes = float_wav(1, 8_000, &[0.25; 64]);
    assert!(decode_audio(bytes.clone(), MediaLimits::new(1, 1).unwrap()).is_err());
    assert!(decode_audio(bytes, MediaLimits::new(1_024, 8).unwrap()).is_err());
    let mut unsupported = float_wav(1, 8_000, &[0.0]);
    // A recognized WAVE container with an unknown codec must not trigger Opus.
    unsupported[20..22].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(decode_audio(unsupported, MediaLimits::default()).is_err());
    let mut truncated = float_wav(1, 8_000, &vec![0.25; 3_000]);
    truncated.truncate(truncated.len() - 100);
    assert!(decode_audio(truncated, MediaLimits::default()).is_err());
}

fn ogg_page(packet: &[u8], flags: u8, sequence: u32, granule: u64) -> Vec<u8> {
    assert!(packet.len() < 255);
    let mut page = b"OggS".to_vec();
    page.extend([0, flags]);
    page.extend(granule.to_le_bytes());
    page.extend(1_u32.to_le_bytes());
    page.extend(sequence.to_le_bytes());
    page.extend([0; 4]);
    page.extend([1, packet.len() as u8]);
    page.extend(packet);
    let mut crc = 0_u32;
    for &byte in &page {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

#[test]
fn test_audio_real_opus_packet_decodes_in_ogg_container() {
    let encoder = audiopus::coder::Encoder::new(
        audiopus::SampleRate::Hz48000,
        audiopus::Channels::Mono,
        audiopus::Application::Audio,
    )
    .unwrap();
    let mut packet = [0; 255];
    let pcm: Vec<f32> = (0..960).map(|i| (i as f32 * 0.057).sin() * 0.2).collect();
    let length = encoder.encode_float(&pcm, &mut packet).unwrap();
    let mut header = b"OpusHead".to_vec();
    header.extend([1, 1]);
    header.extend(0_u16.to_le_bytes());
    header.extend(OPUS_RATE.to_le_bytes());
    header.extend([0, 0, 0]);
    let mut tags = b"OpusTags".to_vec();
    tags.extend([0; 8]);
    let mut bytes = ogg_page(&header, 2, 0, 0);
    bytes.extend(ogg_page(&tags, 0, 1, 0));
    bytes.extend(ogg_page(&packet[..length], 4, 2, 960));
    let decoded = decode_audio(bytes, MediaLimits::default()).unwrap();
    assert_eq!((decoded.sample_rate(), decoded.channels()), (OPUS_RATE, 1));
    assert_eq!(decoded.samples().len(), 960);
    assert!(decoded.samples().iter().all(|value| value.is_finite()));
    assert!(decoded.samples().iter().any(|value| value.abs() > 0.01));
    // OpusHead output gain is part of codec decoding, independent of model policy.
    header[16..18].copy_from_slice(&(6_i16 * 256).to_le_bytes());
    let mut gained = ogg_page(&header, 2, 0, 0);
    gained.extend(ogg_page(&tags, 0, 1, 0));
    gained.extend(ogg_page(&packet[..length], 4, 2, 960));
    let gained = decode_audio(gained, MediaLimits::default()).unwrap();
    let factor = 10_f32.powf(6.0 / 20.0);
    for (&original, &scaled) in decoded.samples().iter().zip(gained.samples()) {
        assert!((scaled - original * factor).abs() < 0.00001);
    }
}
