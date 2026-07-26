use super::config::AudioConfig;
use anyhow::{anyhow, Result};
use std::io::Cursor;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn load_audio_to_pcm(audio_path_or_url: &str, config: &AudioConfig) -> Result<Vec<f32>> {
    let audio_bytes = if audio_path_or_url.starts_with("data:audio/") {
        let comma_pos = audio_path_or_url
            .find(',')
            .ok_or_else(|| anyhow!("Invalid base64 data URI format"))?;
        let base64_raw = &audio_path_or_url[comma_pos + 1..];
        let cleaned: String = base64_raw
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
            .collect();
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&cleaned)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(&cleaned))
            .map_err(|e| anyhow!("Base64 audio decode error: {}", e))?
    } else {
        std::fs::read(audio_path_or_url)
            .map_err(|e| anyhow!("Cannot open audio file '{}': {}", audio_path_or_url, e))?
    };

    let source: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
    let mss = MediaSourceStream::new(source, Default::default());

    let mut hint = Hint::new();
    let lower_path = audio_path_or_url.to_lowercase();

    if lower_path.contains("codecs=opus") || lower_path.contains("audio/opus") {
        hint.with_extension("webm");
    } else if lower_path.ends_with(".mp3") || lower_path.contains("audio/mp3") {
        hint.with_extension("mp3");
    } else if lower_path.ends_with(".m4a")
        || lower_path.ends_with(".mp4")
        || lower_path.contains("audio/mp4")
        || lower_path.contains("audio/m4a")
    {
        hint.with_extension("m4a");
    } else if lower_path.ends_with(".wav") || lower_path.contains("audio/wav") {
        hint.with_extension("wav");
    } else if lower_path.ends_with(".webm") || lower_path.contains("audio/webm") {
        hint.with_extension("webm");
    } else if lower_path.ends_with(".flac") {
        hint.with_extension("flac");
    } else if lower_path.ends_with(".ogg") {
        hint.with_extension("ogg");
    }

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = FormatOptions {
        enable_gapless: false,
        ..Default::default()
    };

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .or_else(|_| {
            let mut webm_hint = Hint::new();
            webm_hint.with_extension("webm");
            let source_fallback: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
            let mss_fallback = MediaSourceStream::new(source_fallback, Default::default());
            symphonia::default::get_probe().format(&webm_hint, mss_fallback, &fmt_opts, &meta_opts)
        })
        .or_else(|_| {
            let mut mkv_hint = Hint::new();
            mkv_hint.with_extension("mkv");
            let source_fallback: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
            let mss_fallback = MediaSourceStream::new(source_fallback, Default::default());
            symphonia::default::get_probe().format(&mkv_hint, mss_fallback, &fmt_opts, &meta_opts)
        })
        .or_else(|_| {
            let source_fallback2: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
            let mss_fallback2 = MediaSourceStream::new(source_fallback2, Default::default());
            symphonia::default::get_probe().format(
                &Hint::new(),
                mss_fallback2,
                &fmt_opts,
                &meta_opts,
            )
        })
        .map_err(|e| anyhow!("Audio format probe error for input: {}", e))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("No supported audio track found"))?;

    let track_id = track.id;
    let src_sample_rate = track.codec_params.sample_rate.unwrap_or(16000);
    let src_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);

    let dec_opts: DecoderOptions = Default::default();
    let mut symphonia_decoder: Option<Box<dyn symphonia::core::codecs::Decoder>> =
        symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .ok();

    let mut audiopus_decoder = if symphonia_decoder.is_none() {
        audiopus::coder::Decoder::new(
            audiopus::SampleRate::Hz48000,
            if src_channels > 1 {
                audiopus::Channels::Stereo
            } else {
                audiopus::Channels::Mono
            },
        )
        .ok()
    } else {
        None
    };

    if symphonia_decoder.is_none() && audiopus_decoder.is_none() {
        return Err(anyhow!(
            "Audio decoder initialization error: Unsupported codec"
        ));
    }

    let mut all_samples: Vec<f32> = Vec::new();
    let mut actual_sample_rate = if audiopus_decoder.is_some() {
        48000
    } else if src_sample_rate > 0 {
        src_sample_rate
    } else {
        48000
    };

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(symphonia::core::errors::Error::ResetRequired) => continue,
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        if let Some(ref mut dec) = symphonia_decoder {
            if let Ok(decoded) = dec.decode(&packet) {
                actual_sample_rate = decoded.spec().rate;
                let spec = *decoded.spec();
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                sample_buf.copy_interleaved_ref(decoded);
                let samples = sample_buf.samples();

                if src_channels > 1 {
                    for chunk in samples.chunks(src_channels) {
                        all_samples.push(chunk.iter().sum::<f32>() / src_channels as f32);
                    }
                } else {
                    all_samples.extend_from_slice(samples);
                }
            }
        } else if let Some(ref mut opus_dec) = audiopus_decoder {
            let mut pcm_buf = vec![0.0f32; 5760 * src_channels];
            if let Ok(num_samples) = opus_dec.decode_float(Some(packet.buf()), &mut pcm_buf, false)
            {
                let count = num_samples as usize;
                let decoded_samples = &pcm_buf[..count * src_channels];
                if src_channels > 1 {
                    for chunk in decoded_samples.chunks(src_channels) {
                        all_samples.push(chunk.iter().sum::<f32>() / src_channels as f32);
                    }
                } else {
                    all_samples.extend_from_slice(decoded_samples);
                }
            }
        }
    }

    if all_samples.is_empty() {
        return Err(anyhow!("Audio decoded to zero samples"));
    }

    let max_amp = all_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_amp > 1.0 {
        for s in &mut all_samples {
            *s /= max_amp;
        }
    }

    Ok(
        if actual_sample_rate != config.sample_rate && actual_sample_rate > 0 {
            linear_resample(&all_samples, actual_sample_rate, config.sample_rate)
        } else {
            all_samples
        },
    )
}

fn linear_resample(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;
        let s0 = samples.get(idx).copied().unwrap_or(0.0);
        let s1 = samples.get(idx + 1).copied().unwrap_or(s0);
        out.push(s0 + frac * (s1 - s0));
    }
    out
}
