//! Container/codec decoding into original interleaved PCM F32. No model preprocessing.

use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedAudio, media_limits::MediaLimits};
use std::io::{Cursor, ErrorKind};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_OPUS},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

// RFC 6716 allows at most 120 ms per Opus packet at this decoder output rate.
const OPUS_RATE: u32 = 48_000;
const OPUS_MAX_PACKET_FRAMES: usize = 5_760;

/// Consumes encoded bytes to avoid cloning the source for container probing.
/// Retains original channels/rate/amplitude; run on the caller's blocking worker.
pub fn decode_audio(bytes: Vec<u8>, limits: MediaLimits) -> Result<DecodedAudio> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    if bytes.is_empty() {
        bail!("Audio source is empty");
    }
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let metadata = MetadataOptions {
        limit_metadata_bytes: Limit::Maximum(0),
        limit_visual_bytes: Limit::Maximum(0),
    };
    let options = FormatOptions {
        enable_gapless: false,
        ..Default::default()
    };
    let mut format = symphonia::default::get_probe()
        .format(&Hint::new(), source, &options, &metadata)
        .context("Unsupported or corrupt audio container")?
        .format;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track.codec_params.sample_rate.is_some() || track.codec_params.channels.is_some()
        })
        .or_else(|| format.default_track())
        .context("No audio track found")?;
    let track_id = track.id;
    let parameters = track.codec_params.clone();

    // Native Opus is used only for a positively identified Opus track, never as
    // a guessed decoder for an unknown/unsupported codec.
    let is_opus = parameters.codec == CODEC_TYPE_OPUS;
    let (channels, opus_gain) = if is_opus {
        let header = parameters
            .extra_data
            .as_deref()
            .context("Opus track is missing its header")?;
        if header.len() < 19
            || !header.starts_with(b"OpusHead")
            || header[8] > 15
            || header[18] != 0
        {
            bail!("Unsupported Opus channel mapping/header");
        }
        let channels = parameters
            .channels
            .context("Opus track is missing channel metadata")?
            .count();
        if usize::from(header[9]) != channels {
            bail!("Opus channel metadata mismatch");
        }
        (
            channels,
            i32::from(i16::from_le_bytes([header[16], header[17]])),
        )
    } else {
        (0, 0)
    };
    let mut opus = if is_opus {
        let layout = match channels {
            1 => audiopus::Channels::Mono,
            2 => audiopus::Channels::Stereo,
            _ => bail!("Native Opus decoding supports only mono or stereo tracks"),
        };
        let decoder = audiopus::coder::Decoder::new(audiopus::SampleRate::Hz48000, layout)?;
        decoder.set_gain(opus_gain)?;
        Some(decoder)
    } else {
        None
    };
    let mut decoder = if is_opus {
        None
    } else {
        Some(
            symphonia::default::get_codecs()
                .make(&parameters, &DecoderOptions::default())
                .context("Unsupported audio codec")?,
        )
    };
    let opus_capacity = if is_opus {
        OPUS_MAX_PACKET_FRAMES * channels
    } else {
        0
    };
    check_pcm_workspace(limits, opus_capacity)?;
    let mut opus_buffer = vec![0.0; opus_capacity];
    let mut samples = Vec::new();
    let mut layout = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error).context("Cannot read audio packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        if let Some(decoder) = decoder.as_mut() {
            let decoded = decoder.decode(&packet).context("Corrupt audio packet")?;
            let spec = *decoded.spec();
            let channel_count = spec.channels.count();
            let capacity = decoded
                .capacity()
                .checked_mul(channel_count)
                .context("Audio packet size overflow")?;
            // Codec-owned decoded buffer, conversion buffer and retained output
            // coexist. This check precedes our conversion/output allocations.
            let workspace = samples
                .len()
                .checked_mul(2)
                .and_then(|n| {
                    capacity
                        .checked_mul(4)
                        .and_then(|packet| n.checked_add(packet))
                })
                .context("Audio workspace overflow")?;
            check_pcm_workspace(limits, workspace)?;
            let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            converted.copy_interleaved_ref(decoded);
            append_pcm(
                &mut samples,
                &mut layout,
                converted.samples(),
                spec.rate,
                channel_count,
                limits,
            )?;
        } else if let Some(decoder) = opus.as_mut() {
            let frames = decoder
                .decode_float(Some(packet.buf()), &mut opus_buffer[..], false)
                .context("Corrupt Opus packet")?;
            let count = frames
                .checked_mul(channels)
                .context("Opus sample count overflow")?;
            let pcm = opus_buffer
                .get(..count)
                .context("Invalid Opus decoded length")?;
            check_pcm_workspace(
                limits,
                samples
                    .len()
                    .checked_add(count)
                    .and_then(|n| n.checked_mul(2))
                    .and_then(|n| n.checked_add(opus_capacity))
                    .context("Opus workspace overflow")?,
            )?;
            append_pcm(&mut samples, &mut layout, pcm, OPUS_RATE, channels, limits)?;
        }
    }
    let (rate, channels) = layout.context("Audio decoded to zero samples")?;
    if let Some(expected_frames) = parameters.n_frames {
        let actual_frames = u64::try_from(samples.len() / usize::from(channels))?;
        if actual_frames < expected_frames {
            bail!("Truncated audio: decoded {actual_frames} frames, expected at least {expected_frames}");
        }
    }
    drop(opus_buffer);
    drop(decoder);
    drop(opus);
    drop(format);
    // Vec -> Arc conversion can temporarily retain both allocations.
    check_pcm_workspace(
        limits,
        samples
            .len()
            .checked_mul(2)
            .context("PCM workspace overflow")?,
    )?;
    Ok(DecodedAudio::new(rate, channels, samples.into())?)
}

fn append_pcm(
    output: &mut Vec<f32>,
    layout: &mut Option<(u32, u16)>,
    pcm: &[f32],
    rate: u32,
    channels: usize,
    limits: MediaLimits,
) -> Result<()> {
    let channels = u16::try_from(channels)?;
    if rate == 0 || channels == 0 || !pcm.len().is_multiple_of(usize::from(channels)) {
        bail!("Invalid decoded audio rate/channels/frame layout");
    }
    if pcm.iter().any(|sample| !sample.is_finite()) {
        bail!("Non-finite decoded audio sample");
    }
    let current = (rate, channels);
    if layout.is_some_and(|previous| previous != current) {
        bail!("Audio rate/channels changed within the stream");
    }
    if pcm.is_empty() {
        return Ok(());
    }
    let next = output
        .len()
        .checked_add(pcm.len())
        .context("PCM sample count overflow")?;
    check_pcm_workspace(
        limits,
        next.checked_mul(2).context("PCM workspace overflow")?,
    )?;
    output.try_reserve_exact(pcm.len())?;
    output.extend_from_slice(pcm);
    *layout = Some(current);
    Ok(())
}

/// Checks PCM workspace sample counts using checked byte arithmetic.
pub(crate) fn check_pcm_workspace(limits: MediaLimits, samples: usize) -> Result<()> {
    let bytes = samples
        .checked_mul(std::mem::size_of::<f32>())
        .context("PCM byte count overflow")?;
    limits.check_decoded_bytes(u64::try_from(bytes)?)?;
    Ok(())
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
