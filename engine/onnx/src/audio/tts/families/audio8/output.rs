//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
use super::{
    package::{Manifest, Protocol},
    AudioSink,
};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use ort::{session::Session, value::TensorRef};

#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    decoder: &mut Session,
    frames: &[Vec<i64>],
    last: bool,
    emitted: &mut usize,
    tempo: &mut cluaiz_media::tempo::Tempo,
    all: &mut Vec<f32>,
    sink: &mut Option<AudioSink<'_>>,
    m: &Manifest,
    p: &Protocol,
    cache_bytes: usize,
    limits: MediaLimits,
) -> Result<()> {
    let start = frames
        .len()
        .saturating_sub(m.stream_context_frames + p.chunk_frames);
    let window = &frames[start..];
    let expected = window
        .len()
        .checked_mul(m.codec_hop_length)
        .context("Audio8 decoder size overflow")?;
    let retained = all
        .len()
        .checked_mul(24)
        .and_then(|n| n.checked_add(expected.checked_mul(16)?))
        .and_then(|n| n.checked_add(cache_bytes.checked_mul(2)?))
        .context("Audio8 output budget overflow")?;
    limits.check_decoded_bytes(u64::try_from(retained)?)?;
    let codes: Vec<_> = (0..m.num_codebooks)
        .flat_map(|book| window.iter().map(move |frame| frame[book]))
        .collect();
    let output = decoder.run(ort::inputs!["codes" => TensorRef::from_array_view(([1, m.num_codebooks, window.len()], codes.as_slice()))?])?;
    let (_, pcm) = output[0].try_extract_tensor::<f32>()?;
    if pcm.len() != expected || pcm.iter().any(|v| !v.is_finite()) {
        bail!("Audio8 codec produced invalid PCM length or values");
    }
    let absolute = start * m.codec_hop_length;
    let begin = emitted
        .checked_sub(absolute)
        .context("Audio8 decoder window lost unplayed samples")?;
    let end = if last {
        pcm.len()
    } else {
        pcm.len()
            .saturating_sub(m.stream_guard_frames * m.codec_hop_length)
    };
    if end > begin {
        let part = tempo.write(&pcm[begin..end])?;
        *emitted = absolute + end;
        send(part, all, sink, m.codec_sample_rate, limits)?;
    }
    Ok(())
}

pub(super) fn send(
    part: Vec<f32>,
    all: &mut Vec<f32>,
    sink: &mut Option<AudioSink<'_>>,
    rate: u32,
    limits: MediaLimits,
) -> Result<()> {
    if part.is_empty() {
        return Ok(());
    }
    limits.check_decoded_bytes(u64::try_from(
        all.len()
            .checked_add(part.len())
            .and_then(|n| n.checked_mul(24))
            .context("Audio8 retained output overflow")?,
    )?)?;
    if let Some(sink) = sink.as_deref_mut() {
        if !sink(cluaiz_media::wav::encode_data_uri(&part, rate, limits)?) {
            bail!("Audio8 speech request cancelled");
        }
    }
    all.extend(part);
    Ok(())
}
