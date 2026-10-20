//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
//! Dimensions and assets of the upstream valid-prefix FP16 export, never guessed KV tensors.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub(super) struct Manifest {
    pub activation_dtype: String,
    pub slow_logits_layout: String,
    pub kv_attention_layout: String,
    pub max_seq_len: usize,
    pub num_layers: usize,
    pub num_fast_layers: usize,
    pub num_codebooks: usize,
    pub n_local_heads: usize,
    pub fast_n_local_heads: usize,
    pub head_dim: usize,
    pub fast_head_dim: usize,
    pub fast_dim: usize,
    pub codebook_size: usize,
    pub semantic_begin_id: i64,
    pub semantic_end_id: i64,
    pub im_end_id: i64,
    pub codec_sample_rate: u32,
    pub codec_hop_length: usize,
    pub stream_context_frames: usize,
    pub stream_guard_frames: usize,
    pub model_fingerprint: String,
}

#[derive(Deserialize)]
pub(super) struct Protocol {
    pub prompt: Vec<String>,
    pub reference_prefix: Vec<String>,
    pub reference_suffix: Vec<String>,
    pub default_speaker: String,
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: usize,
    pub repeat_temperature: f64,
    pub repeat_window: usize,
    pub chunk_frames: usize,
}

pub(super) struct Package {
    pub manifest: Manifest,
    pub protocol: Protocol,
    pub slow: PathBuf,
    pub fast: PathBuf,
    pub decoder: PathBuf,
    pub tokenizer: PathBuf,
}

impl Package {
    pub fn read(dir: &Path) -> Result<Self> {
        let assets = cluaiz_core::model_assets::PackageAssets::from_directory(dir)?
            .context("Audio8 package requires a supported asset manifest")?;
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(dir.join(&assets.manifest))
                .context("Audio8 package requires runtime_manifest.json")?,
        )?;
        let protocol: Protocol = serde_json::from_str(include_str!("protocol.json"))?;
        if manifest.activation_dtype != "float16"
            || manifest.kv_attention_layout != "valid_prefix"
            || manifest.slow_logits_layout != "semantic_then_eos"
        {
            bail!("Unsupported Audio8 export: requires FP16 valid-prefix caches and semantic_then_eos logits");
        }
        for dimension in [
            manifest.max_seq_len,
            manifest.num_layers,
            manifest.num_fast_layers,
            manifest.num_codebooks,
            manifest.n_local_heads,
            manifest.fast_n_local_heads,
            manifest.head_dim,
            manifest.fast_head_dim,
            manifest.fast_dim,
            manifest.codebook_size,
            manifest.codec_hop_length,
            manifest.stream_context_frames,
        ] {
            if dimension == 0 || i64::try_from(dimension).is_err() {
                bail!("Invalid Audio8 manifest dimension");
            }
        }
        if manifest.codec_sample_rate == 0
            || manifest.semantic_begin_id < 0
            || manifest.im_end_id < 0
            || manifest
                .semantic_end_id
                .checked_sub(manifest.semantic_begin_id)
                .and_then(|n| n.checked_add(1))
                != Some(i64::try_from(manifest.codebook_size)?)
            || (manifest.semantic_begin_id..=manifest.semantic_end_id).contains(&manifest.im_end_id)
        {
            bail!("Invalid Audio8 token/rate/precision contract");
        }
        let slow = asset(dir, assets.file("slow")?)?;
        let fast = asset(dir, assets.file("fast")?)?;
        let decoder = asset(dir, assets.file("decoder")?)?;
        let tokenizer = asset(dir, assets.file("tokenizer")?)?;
        Ok(Self {
            manifest,
            protocol,
            slow,
            fast,
            decoder,
            tokenizer,
        })
    }
}

pub(super) fn asset(dir: &Path, relative: &str) -> Result<PathBuf> {
    let root = dir.canonicalize()?;
    let candidate = root
        .join(relative)
        .canonicalize()
        .with_context(|| format!("Audio8 package asset missing: {relative}"))?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
        bail!("Audio8 asset must be a file inside its package");
    }
    Ok(candidate)
}
