//! Measured speed per placement (planner STEP 4). `cluaiz bench` measures a model's generation
//! and prompt speed (tokens/s, llama-bench's `tg` and `pp` tests) on the placement it loaded with,
//! and with `--placements` on every placement the planner can use for it. The planner then loads
//! the placement that generates fastest, as long as it does not read prompts slower, instead of
//! the first one in rule order. Nothing is measured during a normal load, so a load is never
//! slower; a model without measurements keeps the rule order.
//!
//! The numbers are kept with the VRAM measurements (`crate::margin`), in `vram_margins.json`.

use cluaiz_core::{ActiveUserSettings, KvCacheQuantization};
use cluaiz_models::ModelGeometry;

use crate::types::{ExecutionCandidate, PlacementTier};

/// Key prefix of a speed measurement
pub const PREFIX: &str = "speed:";
/// Another placement is loaded only when it was measured at least this much faster (5%): runs of
/// llama-bench on one placement differ by a few percent
pub const FASTER_BY: f64 = 1.05;
/// A measurement still counts for a placement with up to this many blocks (or MoE expert layers)
/// more or less on the GPU: free VRAM moves a little between loads
pub const NEAR_BLOCKS: i64 = 2;

/// What was measured: generation (`tg`, one token at a time) or prompt processing (`pp`, batches)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Test {
    Generation,
    Prompt,
}

impl Test {
    fn tag(self) -> &'static str {
        match self {
            Test::Generation => "tg",
            Test::Prompt => "pp",
        }
    }
}

/// The placement a measurement belongs to
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signature {
    pub tier: PlacementTier,
    pub n_gpu_layers: i32,
    pub n_cpu_moe: u32,
    pub kv_offload: bool,
}

impl Signature {
    pub fn of(candidate: &ExecutionCandidate) -> Self {
        Self {
            tier: candidate.tier,
            n_gpu_layers: candidate.n_gpu_layers,
            n_cpu_moe: candidate.n_cpu_moe,
            kv_offload: candidate.kv_offload,
        }
    }
}

/// Everything but the GPU block counts: model, load settings, context, KV type, kind of placement
fn family(test: Test, geometry: &ModelGeometry, settings: &ActiveUserSettings, n_ctx: u32, kv: KvCacheQuantization, s: &Signature) -> String {
    format!(
        "{PREFIX}{}:{}:ctx{}:{:?}:{:?}:kvo{}:",
        test.tag(),
        crate::margin::key(geometry, settings),
        n_ctx,
        kv,
        s.tier,
        s.kv_offload as u8
    )
}

pub fn key(test: Test, geometry: &ModelGeometry, settings: &ActiveUserSettings, n_ctx: u32, kv: KvCacheQuantization, s: &Signature) -> String {
    format!("{}ngl{}:moe{}", family(test, geometry, settings, n_ctx, kv, s), s.n_gpu_layers, s.n_cpu_moe)
}

/// Keeps a measured speed in tokens/s (replaces an earlier one of the same placement and test)
#[allow(clippy::too_many_arguments)]
pub fn record(test: Test, geometry: &ModelGeometry, settings: &ActiveUserSettings, n_ctx: u32, kv: KvCacheQuantization, s: &Signature, tokens_per_s: f64) {
    if tokens_per_s.is_finite() && tokens_per_s > 0.0 {
        crate::margin::set_key(&key(test, geometry, settings, n_ctx, kv, s), tokens_per_s);
    }
}

/// The measured speed of this placement, or of the nearest measured one of the same kind within
/// [`NEAR_BLOCKS`]
pub fn measured(test: Test, geometry: &ModelGeometry, settings: &ActiveUserSettings, n_ctx: u32, kv: KvCacheQuantization, s: &Signature) -> Option<f64> {
    let family = family(test, geometry, settings, n_ctx, kv, s);
    let mut nearest: Option<(i64, f64)> = None;
    for (key, value) in crate::margin::snapshot() {
        let Some(rest) = key.strip_prefix(&family) else { continue };
        let Some((ngl, moe)) = parse_counts(rest) else { continue };
        // -1 (every block) and 0 (CPU) are exact placements, never "near" a count
        if (ngl <= 0 || s.n_gpu_layers <= 0) && ngl != s.n_gpu_layers as i64 {
            continue;
        }
        let distance = (ngl - s.n_gpu_layers as i64).abs() + (moe - s.n_cpu_moe as i64).abs();
        if distance <= NEAR_BLOCKS && nearest.map_or(true, |(d, _)| distance < d) {
            nearest = Some((distance, value));
        }
    }
    nearest.map(|(_, v)| v)
}

fn parse_counts(rest: &str) -> Option<(i64, i64)> {
    let (ngl, moe) = rest.strip_prefix("ngl")?.split_once(":moe")?;
    Some((ngl.parse().ok()?, moe.parse().ok()?))
}

/// STEP 4: the index of the placement to load. `rule` is the rule-order pick among `candidates`;
/// another usable candidate replaces it when both were measured, it generates [`FASTER_BY`]
/// faster, and it does not read prompts more than that much slower (when both prompt speeds are
/// known). `usable` says which candidates may be loaded at all.
pub fn fastest(candidates: &[ExecutionCandidate], rule: usize, usable: impl Fn(&ExecutionCandidate) -> bool) -> usize {
    let Some(rule_speed) = candidates[rule].measured_tokens_per_s else { return rule };
    let rule_prompt = candidates[rule].measured_prompt_tokens_per_s;
    candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| usable(c))
        .filter(|(_, c)| match (c.measured_prompt_tokens_per_s, rule_prompt) {
            (Some(pp), Some(rule_pp)) => pp * FASTER_BY >= rule_pp,
            _ => true,
        })
        .filter_map(|(i, c)| c.measured_tokens_per_s.map(|tps| (i, tps)))
        .filter(|(_, tps)| *tps >= rule_speed * FASTER_BY)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(rule, |(i, _)| i)
}
