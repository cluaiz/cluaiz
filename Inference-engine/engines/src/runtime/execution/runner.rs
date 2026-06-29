//! ═══════════════════════════════════════════════════════════════════════
//!   Engine: Universal Runner (cluaiz)
//! ═══════════════════════════════════════════════════════════════════════

use anyhow::Result;
use crate::runtime::execution::sampler::CoreSampler;
use cluaiz_shared::ModelWeightsWrapper;



#[derive(Debug, Clone)]
pub struct cluaizMetrics {
    pub ttft_ms: f64,
    pub tps: f64,
    pub total_tokens: usize,
    pub total_time_ms: f64,
}

pub struct cluaizRunner {
    pub model: ModelWeightsWrapper,

    pub sampler: CoreSampler,
    pub bos_token_id: Option<u32>,
}

impl cluaizRunner {
    pub fn new(model: ModelWeightsWrapper, sampler: CoreSampler, bos_token_id: Option<u32>) -> Self {
        Self { model, sampler, bos_token_id }
    }

    /// 🔗 Instant Recall: Injects Core cluaiz signals before generation.
    pub fn inject_Core_signals(&mut self, signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>) -> Result<()> {
        self.model.inject_signals(signals)
    }

    pub fn generate(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> Result<cluaizMetrics> {
        // 🛰️ cluaiz BOOSTER SYNC: Load truth from Governor before generation
        let booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
        self.model.apply_booster(&booster)?;
        
        // 🌊 Liquid Mode Linkage
        if booster.turbo_quant == cluaiz_shared::hardware::schema::booster::FeatureState::On {
            self.model.set_liquid_mode(true)?;
        }

        let start_time = std::time::Instant::now();
        
        let token_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count_clone = std::sync::Arc::clone(&token_count);

        // 🧪 ARCHER V5.3 CONVERGENCE: Delegating generation to the kernel
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

        Ok(cluaizMetrics {
            ttft_ms: 0.0, // Model TTFT placeholder for now
            tps,
            total_tokens: actual_tokens,
            total_time_ms: total_duration.as_secs_f64() * 1000.0,
        })
    }
}
