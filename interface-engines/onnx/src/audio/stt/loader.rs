use super::super::config::AudioConfig;
use anyhow::{anyhow, Result};
use std::io::Cursor;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn load_audio_to_pcm(audio_path_or_url: &str, config: &AudioConfig) -> Result<Vec<f32>> {
    let t_start_a = std::time::Instant::now();
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
        let clean_path = audio_path_or_url.replace('\\', "/");
        std::fs::read(&clean_path)
            .or_else(|_| std::fs::read(audio_path_or_url))
            .map_err(|e| anyhow!("Cannot open audio file '{}': {}", audio_path_or_url, e))?
    };
    println!("⏱️ [BENCHMARK] Step A.1 - Disk/Base64 Read Time: {:?}", t_start_a.elapsed());

    let t_start_probe = std::time::Instant::now();
    let mut hint = Hint::new();
    let lower_path = audio_path_or_url.to_lowercase();

    if lower_path.ends_with(".webm") || lower_path.contains("audio/webm") || lower_path.contains("codecs=opus") {
        hint.with_extension("mkv");
    } else if audio_bytes.len() >= 4 {
        if &audio_bytes[0..4] == b"RIFF" {
            hint.with_extension("wav");
        } else if &audio_bytes[0..4] == b"\x1a\x45\xdf\xa3" {
            hint.with_extension("mkv");
        } else if &audio_bytes[0..3] == b"ID3" || (audio_bytes[0] == 0xFF && (audio_bytes[1] & 0xE0) == 0xE0) {
            hint.with_extension("mp3");
        } else if lower_path.ends_with(".m4a") || lower_path.ends_with(".mp4") || lower_path.contains("audio/mp4") {
            hint.with_extension("m4a");
        } else if lower_path.ends_with(".flac") {
            hint.with_extension("flac");
        } else if lower_path.ends_with(".ogg") {
            hint.with_extension("ogg");
        }
    }

    let source: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
    let mss = MediaSourceStream::new(source, Default::default());

    let meta_opts = MetadataOptions {
        limit_metadata_bytes: symphonia::core::meta::Limit::Maximum(0),
        limit_visual_bytes: symphonia::core::meta::Limit::Maximum(0),
    };
    let fmt_opts: FormatOptions = FormatOptions {
        enable_gapless: false,
        ..Default::default()
    };

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .or_else(|_| {
            let source_fallback: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
            let mss_fallback = MediaSourceStream::new(source_fallback, Default::default());
            let mut mkv_hint = Hint::new();
            mkv_hint.with_extension("mkv");
            symphonia::default::get_probe().format(&mkv_hint, mss_fallback, &fmt_opts, &meta_opts)
        })
        .or_else(|_| {
            let source_fallback: Box<dyn MediaSource> = Box::new(Cursor::new(audio_bytes.clone()));
            let mss_fallback = MediaSourceStream::new(source_fallback, Default::default());
            let empty_hint = Hint::new();
            symphonia::default::get_probe().format(&empty_hint, mss_fallback, &fmt_opts, &meta_opts)
        })
        .map_err(|e| anyhow!("Unsupported audio format or corrupt audio file: {}", e))?;
    println!("⏱️ [BENCHMARK] Step A.2 - Symphonia Probing Time: {:?}", t_start_probe.elapsed());

    let t_start_packet_loop = std::time::Instant::now();
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .or_else(|| format.tracks().iter().find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL))
        .or_else(|| format.default_track())
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
            Err(_) => continue,
        };

        if packet.track_id() != track_id {
            continue;
        }

        if let Some(ref mut dec) = symphonia_decoder {
            match dec.decode(&packet) {
                Ok(decoded) => {
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
                Err(_) => {
                    if let Some(ref mut opus_dec) = audiopus_decoder {
                        let mut pcm_buf = vec![0.0f32; 5760 * src_channels];
                        if let Ok(num_samples) = opus_dec.decode_float(Some(packet.buf()), &mut pcm_buf, false) {
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
    println!("⏱️ [BENCHMARK] Step A.3 - Symphonia Packet Decoding Loop Time: {:?}", t_start_packet_loop.elapsed());

    if all_samples.is_empty() {
        return Err(anyhow!("Audio decoded to zero samples"));
    }

    let max_amp = all_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_amp > 1.0 {
        for s in &mut all_samples {
            *s /= max_amp;
        }
    }

    let t_start_resample = std::time::Instant::now();
    let resampled = if actual_sample_rate != config.sample_rate && actual_sample_rate > 0 {
        let res = linear_resample(&all_samples, actual_sample_rate, config.sample_rate);
        println!("⏱️ [BENCHMARK] Step A.4 - Rayon Multi-Core Resampling Time: {:?} ({}Hz -> {}Hz)", t_start_resample.elapsed(), actual_sample_rate, config.sample_rate);
        res
    } else {
        all_samples
    };

    Ok(resampled)
}

use rayon::prelude::*;

fn linear_resample(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    
    (0..out_len)
        .into_par_iter()
        .map(|i| {
            let src_pos = i as f64 * ratio;
            let idx = src_pos as usize;
            let frac = (src_pos - idx as f64) as f32;
            let s0 = samples.get(idx).copied().unwrap_or(0.0);
            let s1 = samples.get(idx + 1).copied().unwrap_or(s0);
            s0 + frac * (s1 - s0)
        })
        .collect()
}
