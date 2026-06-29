//! ═══════════════════════════════════════════════════════════════════════
//!   Engine: Core Sampler (cluaiz) — Native Refactor
//! ═══════════════════════════════════════════════════════════════════════

use rand::{distr::Distribution, SeedableRng};
use serde::{Serialize, Deserialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceMode { Turbo, Classic, Auto }

#[derive(Debug, Clone)]
pub struct CoreSampler {
    pub temperature: f32,
    pub top_p: f32,
    pub repeat_penalty: f32,
    pub seed: u64,
}

impl CoreSampler {
    pub fn new(seed: u64, temp: f32, top_p: f32, penalty: f32) -> Self {
        Self { seed, temperature: temp, top_p, repeat_penalty: penalty }
    }

    /// Dispatch sampling based on InferenceMode.
    /// In production, `valid_tokens` is provided dynamically by the active `GrammarMasker` FSM state.
    pub fn sample(&self, logits: &mut [f32], mode: &InferenceMode, masker: Option<&crate::runtime::execution::logit_processor::GrammarMasker>, valid_tokens: Option<&[u32]>) -> Result<u32> {
        // 🛡️ LogitProcessor Phase: Pre-sampling constraint injection
        if let Some(m) = masker {
            if let Some(vt) = valid_tokens {
                m.mask_logits(logits, vt)?;
            }
        }

        match mode {
            InferenceMode::Turbo => self.sample_greedy(logits),
            InferenceMode::Classic => self.sample_creative(logits),
            InferenceMode::Auto => self.sample_greedy(logits),
        }
    }

    /// Pure Greedy Sampling (Argmax).
    pub fn sample_greedy(&self, logits: &[f32]) -> Result<u32> {
        let mut best_idx = 0;
        let mut best_val = f32::MIN;

        for (i, &val) in logits.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_idx = i;
            }
        }
        Ok(best_idx as u32)
    }

    /// Creative Sampling (Top-P).
    pub fn sample_creative(&self, logits: &[f32]) -> Result<u32> {
        if self.temperature == 0.0 {
            return self.sample_greedy(logits);
        }

        // Apply Temperature
        let mut prs: Vec<f32> = logits.iter()
            .map(|&l| (l / self.temperature).exp())
            .collect();
        
        // Softmax
        let sum: f32 = prs.iter().sum();
        for p in prs.iter_mut() {
            *p /= sum;
        }

        let mut prs_with_index: Vec<(usize, f32)> = prs.into_iter().enumerate().collect();
        
        // Sort for Top-P
        prs_with_index.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        let mut cumulative_p = 0.0;
        let mut top_p_count = 0;
        for (_, p) in prs_with_index.iter() {
            cumulative_p += p;
            top_p_count += 1;
            if cumulative_p >= self.top_p { break; }
        }
        
        let prs_with_index = &prs_with_index[..top_p_count];
        let total_p: f32 = prs_with_index.iter().map(|(_, p)| p).sum();
        
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed);
        let distr = rand::distr::weighted::WeightedIndex::new(
            prs_with_index.iter().map(|(_, p)| p / total_p)
        ).map_err(|e| anyhow::anyhow!("Sampling Error: {}", e))?;
        
        Ok(prs_with_index[distr.sample(&mut rng)].0 as u32)
    }
}
