//! archer-shared: Common traits and types for the  Engine ecosystem.
#![allow(warnings)]

#[macro_export]
macro_rules! dev_info {
    ($($arg:tt)*) => {
        // Internal engine diagnostic — only visible with RUST_LOG=debug
        // Do NOT use println! here — it pollutes the CLI user interface
        tracing::debug!($($arg)*);
    };
}


pub struct cluaizdbNA;

use std::sync::atomic::{AtomicBool, Ordering};

/// 🛑 Global Cancellation Signal for Graceful Interrupts
pub static GLOBAL_CANCEL_SIGNAL: AtomicBool = AtomicBool::new(false);
/// ⚡ Global Signal for Native In-Flight Logit Clamping (Skip Thinking)
pub static GLOBAL_SKIP_THINKING_SIGNAL: AtomicBool = AtomicBool::new(false);

pub mod hardware;
pub mod metadata;
pub mod neural;
pub mod prompting;
pub mod backend;
pub mod neural_core;
pub mod utils;
pub mod skills;

// ── Business Logic (Unified from shared) ──
pub mod environment;

pub use hardware::{governor::*, telemetry::*};
pub use metadata::dna::*;
pub use prompting::templater::*;
pub use backend::{context::*, traits::*, signature::*};
pub use neural_core::NeuralResult;

/// 🏛️ cluaizLinkerPlaceholder: Used during Phase 1 to verify the Dynamic Linker Handshake.
pub struct cluaizLinkerPlaceholder;

// Ensure placeholder is Send + Sync for the Orchestrator's type requirements
unsafe impl Send for cluaizLinkerPlaceholder {}
unsafe impl Sync for cluaizLinkerPlaceholder {}

impl crate::backend::traits::UnifiedBackend for cluaizLinkerPlaceholder {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> std::result::Result<String, String> {
        Err("✅ SOVEREIGN LINKER: Phase 1 Handshake Success. Real inference pending Phase 2.".to_string())
    }
    fn prefill(&mut self, _prompt: &str) -> anyhow::Result<()> { Ok(()) }
    fn evaluate_tps(&self) -> f64 { 0.0 }
}

impl crate::backend::traits::cluaizInference for cluaizLinkerPlaceholder {
    fn forward_raw(&mut self, _input_ids: &[u32], _pos: usize) -> anyhow::Result<Vec<f32>> {
        Err(anyhow::anyhow!("Handshake Placeholder"))
    }
    fn generate_stream(
        &mut self,
        _prompt: &str,
        _max_tokens: usize,
        _callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("✅ SOVEREIGN LINKER: Handshake Complete. Ready for Phase 2."))
    }
}
