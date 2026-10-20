//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
//! Stateful Slow AR -> Fast AR -> codec synthesis for the inspected Audio8 FP16-cache export.
#[path = "audio8/cache.rs"]
mod cache;
#[path = "audio8/output.rs"]
mod output;
#[path = "audio8/package.rs"]
mod package;
#[path = "audio8/sampling.rs"]
mod sampling;
#[path = "audio8/steps.rs"]
mod steps;
#[cfg(test)]
#[path = "audio8/tests.rs"]
mod tests;
#[path = "audio8/voice.rs"]
mod voice;

use super::super::tts_router::AudioSink;
use anyhow::{bail, Context, Result};
use cache::Caches;
use cluaiz_core::media_limits::MediaLimits;
use ort::{session::Session, value::TensorElementType};
use package::{Manifest, Package, Protocol};
use std::path::Path;
use tokenizers::Tokenizer;

pub(crate) struct Runtime {
    dir: std::path::PathBuf,
    manifest: Manifest,
    protocol: Protocol,
    tokenizer: Tokenizer,
    slow: Session,
    fast: Session,
    decoder: Session,
}

impl Runtime {
    pub fn load(engine: &crate::OnnxEngine, dir: &Path) -> Result<Self> {
        let p = Package::read(dir)?;
        let tokenizer = Tokenizer::from_file(&p.tokenizer)
            .map_err(|e| anyhow::anyhow!("Audio8 tokenizer: {e}"))?;
        // The reference runtime requests ORT_ENABLE_ALL. Level3 is only ENABLE_LAYOUT:
        // on the inspected INT4/FP16 export it changes the very first semantic logits,
        // causing noise and a missing speech EOS (see the recorded differential proof).
        let optimization = ort::session::builder::GraphOptimizationLevel::All;
        let slow = engine.build_session_with_optimization(&p.slow, optimization)?;
        let fast = engine.build_session_with_optimization(&p.fast, optimization)?;
        let decoder = engine.build_session_with_optimization(&p.decoder, optimization)?;
        let m = &p.manifest;
        for (session, name, element, shape) in [
            (
                &slow,
                "codes",
                TensorElementType::Int64,
                vec![1, (m.num_codebooks + 1) as i64, -1],
            ),
            (&slow, "input_pos", TensorElementType::Int64, vec![-1]),
            (
                &fast,
                "slow_hidden",
                TensorElementType::Float16,
                vec![1, 1, m.fast_dim as i64],
            ),
            (&fast, "token_id", TensorElementType::Int64, vec![1, 1]),
            (&fast, "use_slow_hidden", TensorElementType::Bool, vec![1]),
            (&fast, "input_pos", TensorElementType::Int64, vec![1]),
            (
                &decoder,
                "codes",
                TensorElementType::Int64,
                vec![1, m.num_codebooks as i64, -1],
            ),
        ] {
            let input = session
                .inputs()
                .iter()
                .find(|i| i.name() == name)
                .with_context(|| format!("Audio8 graph input missing: {name}"))?;
            crate::media::validate_tensor(input.dtype(), element, &shape)?;
        }
        if slow.inputs().len() != 2 + m.num_layers * 2
            || fast.inputs().len() != 4 + m.num_fast_layers * 2
            || decoder.inputs().len() != 1
            || decoder.outputs().len() != 1
        {
            bail!("Unsupported Audio8 graph input/output inventory");
        }
        Ok(Self {
            dir: dir.to_owned(),
            manifest: p.manifest,
            protocol: p.protocol,
            tokenizer,
            slow,
            fast,
            decoder,
        })
    }

    fn prompt(
        &self,
        text: &str,
        voice: Option<&str>,
        limits: MediaLimits,
    ) -> Result<(Vec<i64>, usize)> {
        let cleaned = clean_text(text);
        if cleaned.is_empty() {
            bail!("Audio8 requires nonempty speech text");
        }
        let reference = voice
            .map(|name| voice::load(&self.dir, name, &self.manifest, limits))
            .transpose()?;
        let reference_text = reference.as_ref().map(|r| {
            let text = clean_text(&r.text);
            if text.split("<|speaker:").skip(1).any(|part| {
                part.split_once("|>")
                    .is_some_and(|(id, _)| id.parse::<usize>().is_ok())
            }) {
                text
            } else {
                format!("{}{text}", self.protocol.default_speaker)
            }
        });
        let encode = |parts: &[String]| -> Result<Vec<i64>> {
            let mut ids = Vec::new();
            for part in parts {
                let piece = match part.as_str() {
                    "{text}" => &cleaned,
                    "{reference_text}" => {
                        reference_text.as_ref().context("Missing reference text")?
                    }
                    _ => part,
                };
                ids.extend(
                    self.tokenizer
                        .encode(piece.as_str(), false)
                        .map_err(|e| anyhow::anyhow!("Audio8 text encoding: {e}"))?
                        .get_ids()
                        .iter()
                        .map(|&v| i64::from(v)),
                );
            }
            Ok(ids)
        };
        let mut ids;
        let reference_begin;
        if let Some(ref reference) = reference {
            ids = encode(&self.protocol.reference_prefix)?;
            reference_begin = ids.len();
            ids.extend(
                reference.codes[..reference.frames]
                    .iter()
                    .map(|&id| self.manifest.semantic_begin_id + id),
            );
            ids.extend(encode(&self.protocol.reference_suffix)?);
        } else {
            ids = encode(&self.protocol.prompt)?;
            reference_begin = 0;
        }
        let len = ids.len();
        if len == 0 || len >= self.manifest.max_seq_len {
            bail!("Audio8 text exceeds its package context; shorten the text");
        }
        let count = len
            .checked_mul(self.manifest.num_codebooks + 1)
            .context("Audio8 prompt size overflow")?;
        limits.check_decoded_bytes(u64::try_from(
            count
                .checked_mul(8)
                .context("Audio8 prompt byte overflow")?,
        )?)?;
        // Text positions carry no acoustic books. This is prompt padding, never generated audio.
        ids.resize(count, 0);
        if let Some(reference) = reference {
            for book in 0..self.manifest.num_codebooks {
                let begin = (book + 1) * len + reference_begin;
                ids[begin..begin + reference.frames].copy_from_slice(
                    &reference.codes[book * reference.frames..(book + 1) * reference.frames],
                );
            }
        }
        Ok((ids, len))
    }

    pub fn synthesize(
        &mut self,
        text: &str,
        voice: Option<&str>,
        speed: f32,
        limits: MediaLimits,
        mut sink: Option<AudioSink<'_>>,
    ) -> Result<String> {
        let m = &self.manifest;
        let p = &self.protocol;
        let (prompt, prompt_len) = self.prompt(text, voice, limits)?;
        let slow_bytes = Caches::bytes(m.num_layers, m.n_local_heads, m.max_seq_len, m.head_dim)?;
        let fast_bytes = Caches::bytes(
            m.num_fast_layers,
            m.fast_n_local_heads,
            m.num_codebooks,
            m.fast_head_dim,
        )?;
        let cache_bytes = slow_bytes
            .checked_add(fast_bytes)
            .context("Audio8 cache budget overflow")?;
        // Input caches and returned deltas can coexist. ORT graph workspace is separate.
        limits.check_decoded_bytes(u64::try_from(
            cache_bytes
                .checked_mul(2)
                .context("Audio8 cache budget overflow")?,
        )?)?;
        let mut slow_cache = Caches::new(
            m.num_layers,
            m.n_local_heads,
            m.max_seq_len,
            m.head_dim,
            limits,
        )?;
        let mut fast_cache = Caches::new(
            m.num_fast_layers,
            m.fast_n_local_heads,
            m.num_codebooks,
            m.fast_head_dim,
            limits,
        )?;
        slow_cache.validate(&self.slow)?;
        fast_cache.validate(&self.fast)?;
        let positions: Vec<_> = (0..prompt_len).map(|n| n as i64).collect();
        let (mut logits, mut hidden) =
            steps::slow(&mut self.slow, &prompt, &positions, &mut slow_cache, m)?;
        let mut rng = rand::thread_rng();
        let mut previous = Vec::new();
        let mut frames: Vec<Vec<i64>> = Vec::new();
        let mut all_pcm = Vec::new();
        let mut emitted = 0usize;
        let mut tempo = cluaiz_media::tempo::Tempo::new(m.codec_sample_rate, speed, limits)?;
        let mut ended = false;
        for step in 0..m.max_seq_len - prompt_len {
            let normal = sampling::sample(&logits, p.temperature, p.top_p, p.top_k, &mut rng)?;
            // Upstream draws both samples before applying its repetition rule.
            let high = sampling::sample(&logits, p.repeat_temperature, p.top_p, p.top_k, &mut rng)?;
            let semantic = if normal < m.codebook_size && previous.contains(&normal) {
                high
            } else {
                normal
            };
            if semantic == m.codebook_size {
                ended = true;
                break;
            }
            previous.push(semantic);
            if previous.len() > p.repeat_window {
                previous.remove(0);
            }
            fast_cache.reset();
            let _priming = steps::fast(&mut self.fast, &hidden, 0, true, 0, &mut fast_cache, m)?;
            let mut token = i64::try_from(semantic)?;
            let mut frame = vec![token];
            for position in 1..m.num_codebooks {
                let fast_logits = steps::fast(
                    &mut self.fast,
                    &hidden,
                    token,
                    false,
                    position,
                    &mut fast_cache,
                    m,
                )?;
                token = i64::try_from(sampling::sample(
                    &fast_logits,
                    p.temperature,
                    p.top_p,
                    p.top_k,
                    &mut rng,
                )?)?;
                frame.push(token);
            }
            frames.push(frame);
            if frames.len() % p.chunk_frames == 0 {
                output::emit(
                    &mut self.decoder,
                    &frames,
                    false,
                    &mut emitted,
                    &mut tempo,
                    &mut all_pcm,
                    &mut sink,
                    m,
                    p,
                    cache_bytes,
                    limits,
                )?;
            }
            let mut column = vec![m.semantic_begin_id + i64::try_from(semantic)?];
            column.extend(frames.last().context("Missing generated Audio8 frame")?);
            (logits, hidden) = steps::slow(
                &mut self.slow,
                &column,
                &[(prompt_len + step) as i64],
                &mut slow_cache,
                m,
            )?;
        }
        if !ended {
            bail!("Audio8 exhausted its context before end-of-speech; no complete utterance was produced");
        }
        if frames.is_empty() {
            bail!("Audio8 produced no codec frames");
        }
        output::emit(
            &mut self.decoder,
            &frames,
            true,
            &mut emitted,
            &mut tempo,
            &mut all_pcm,
            &mut sink,
            m,
            p,
            cache_bytes,
            limits,
        )?;
        output::send(
            tempo.finish()?,
            &mut all_pcm,
            &mut sink,
            m.codec_sample_rate,
            limits,
        )?;
        cluaiz_media::wav::encode_data_uri(&all_pcm, m.codec_sample_rate, limits)
    }
}

fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn validate_package(dir: &Path) -> Result<()> {
    Package::read(dir).map(|_| ())
}

/// Reference voices are package assets, not arbitrary client paths.
pub fn registered_voices(dir: &Path) -> Vec<super::super::languages::Voice> {
    let Ok(package) = Package::read(dir) else {
        return Vec::new();
    };
    std::fs::read_dir(dir.join("voices"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            package::asset(dir, &format!("voices/{name}/codes.npy")).ok()?;
            let meta = package::asset(dir, &format!("voices/{name}/meta.json")).ok()?;
            cluaiz_core::Settings::current()
                .media
                .limits
                .check_input_bytes(std::fs::metadata(&meta).ok()?.len())
                .ok()?;
            let meta: serde_json::Value =
                serde_json::from_slice(&std::fs::read(meta).ok()?).ok()?;
            if meta["model_fingerprint"].as_str()
                != Some(package.manifest.model_fingerprint.as_str())
            {
                return None;
            }
            meta["reference_text"]
                .as_str()
                .filter(|text| !text.trim().is_empty())?;
            Some(super::super::languages::Voice {
                id: name,
                language: meta["language"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

pub fn execute(
    engine: &crate::OnnxEngine,
    text: &str,
    voice: Option<&str>,
    speed: f32,
    limits: MediaLimits,
    sink: Option<AudioSink<'_>>,
) -> Result<String> {
    engine
        .audio8_runtime
        .as_ref()
        .context("Audio8 package was not loaded through the typed audio loader")?
        .lock()
        .map_err(|_| anyhow::anyhow!("Audio8 runtime mutex poisoned"))?
        .synthesize(text, voice, speed, limits, sink)
}
