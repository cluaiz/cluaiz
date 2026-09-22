//! Unified Sampling Logic: Hardware-aware and architecture-agnostic.

#[derive(Debug, Clone, Copy)]
pub enum SamplingStrategy {
    Greedy,
    TopP(f32),
    TopK(u32),
}

pub struct SamplingParams {
    pub strategy: SamplingStrategy,
    pub temperature: f32,
    pub penalty_repeat: f32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            strategy: SamplingStrategy::Greedy,
            temperature: 0.8,
            penalty_repeat: 1.1,
        }
    }
}
