//! Dynamic Neural Configuration: Resolves architecture-specific parameters 
//! by reconciling Structural DNA with actual Hardware Governor limits.

use crate::metadata::dna::StructuralDNA;
use crate::hardware::HardwareGovernor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedNeuralParams {
    pub n_ctx: u32,
    pub batch_size: u32,
    pub sliding_window: Option<u32>,
    pub rope_freq_base: f32,
    pub threads: u32,
    pub gpu_layers: u32,
}

pub struct NeuralConfig;

impl NeuralConfig {
    /// Reconciles model DNA with real-time hardware constraints.
    /// This eliminates hardcoded values in engine implementations.
    pub fn resolve(dna: &StructuralDNA) -> ResolvedNeuralParams {
        let sys_control = HardwareGovernor::load_system_control();
        
        // 1. Dynamic Context Window (Single Source of Truth)
        let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
        let n_ctx = HardwareGovernor::negotiate_vram_envelope_with_optimization(dna, &opt_control) as u32;

        // 2. Dynamic Batch Size (Adaptive Policy)
        let batch_size = 512; // Static base for Phase 1

        // 3. Sliding Window (Derived from DNA or hardware sense)
        let sliding_window = dna.dynamic_attributes.get("sliding_window")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|n| n as u32);

        // 4. RoPE Frequency (Stable defaults from architecture)
        let rope_freq_base = dna.dynamic_attributes.get("rope_freq_base")
            .and_then(|v| v.parse::<f64>().ok())
            .map(|f| f as f32)
            .unwrap_or(10000.0);

        // 5. Threading (Sensed from Real Silicon Physical Cores)
        let threads = sys_control.as_ref().ok()
            .map(|sc| sc.silicon_truth.cpu.physical_cores)
            .unwrap_or(8);

        // 6. GPU Offloading (Authoritative DNA)
        let gpu_layers = dna.layer_count.map(|l| l as u32).unwrap_or(0);

        ResolvedNeuralParams {
            n_ctx,
            batch_size,
            sliding_window,
            rope_freq_base,
            threads,
            gpu_layers,
        }
    }
}
