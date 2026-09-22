//! ═══════════════════════════════════════════════════════════════════════
//!   Engine: Core Execution Runner
//! ═══════════════════════════════════════════════════════════════════════

use anyhow::Result;
use crate::runtime::execution::sampler::CoreSampler;
use engine_core::ModelWeightsWrapper;

#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    pub ttft_ms: f64,
    pub tps: f64,
    pub total_tokens: usize,
    pub total_time_ms: f64,
}

pub struct EngineRunner {
    pub model: ModelWeightsWrapper,
    pub sampler: CoreSampler,
    pub bos_token_id: Option<u32>,
}

impl EngineRunner {
    pub fn new(model: ModelWeightsWrapper, sampler: CoreSampler, bos_token_id: Option<u32>) -> Self {
        Self { model, sampler, bos_token_id }
    }

    /// Injects Core kernel signals before generation.
    pub fn inject_Core_signals(&mut self, signals: Vec<engine_core::hardware::memory::kv_cache::stitching::KernelSignal>) -> Result<()> {
        self.model.inject_signals(signals)
    }

    pub fn generate(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> Result<ExecutionMetrics> {
        // OPTIMIZATION SYNC: Load truth from Governor before generation
        let optimization = engine_core::hardware::governor::HardwareGovernor::load_optimization_settings().unwrap_or_default();
        self.model.apply_optimization(&optimization)?;
        
        // 🌊 Liquid Mode Linkage
        if optimization.kv_cache_quantization != engine_core::hardware::schema::optimization::KvCacheQuantization::Kv16 {
            self.model.set_liquid_mode(true)?;
        }

        let start_time = std::time::Instant::now();
        
        let token_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count_clone = std::sync::Arc::clone(&token_count);

        // Delegating generation to the kernel
        self.model.generate_stream(
            prompt,
            max_tokens,

            Box::new(move |t| -> bool {
                count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = callback(t);
                true
            }),
        )?;

        let total_duration = start_time.elapsed();
        let total_seconds = total_duration.as_secs_f64();
        let actual_tokens = token_count.load(std::sync::atomic::Ordering::SeqCst);

        // Safe division logic to avoid infinite TPS anomalies
        let tps = if total_seconds > 0.0 && actual_tokens > 0 {
            actual_tokens as f64 / total_seconds
        } else {
            0.0
        };

        Ok(ExecutionMetrics {
            ttft_ms: 0.0, // Model TTFT placeholder for now
            tps,
            total_tokens: actual_tokens,
            total_time_ms: total_duration.as_secs_f64() * 1000.0,
        })
    }
}
