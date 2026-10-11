//! 🧠 Cluaiz Core Engine Library (`engine/core`)
//! Provides centralized environment resolution, units, contracts, and cached settings.

pub mod env;
pub mod settings;
pub mod units;
pub mod contracts;

pub use env::{EnvironmentManager, EnvironmentMode};
pub use settings::{
    Settings, ActiveUserSettings, OptimizationControl, FeatureState, KvCacheQuantization, ContextShiftingMode,
    ContextSetting, SamplerDefaults, LlamaLoadParams, set_active_context, active_context_tokens, active_model_path,
    DEFAULT_SAFE_VRAM_BUFFER_GB, MIN_SAFE_VRAM_BUFFER_GB, DEFAULT_SAFE_RAM_BUFFER_GB, MIN_SAFE_RAM_BUFFER_GB,
};
pub use units::{BYTE, KIB, MIB, GIB, GIB_F64, MIB_F64, bytes_to_gib, bytes_to_mib, gib_to_bytes, mib_to_bytes};
pub use contracts::{UnifiedBackend, StreamingInference, StreamCallback, Modality, EngineError, EmbeddingDriver};

use std::sync::atomic::AtomicBool;

/// 🛑 Global Cancellation Signal for Graceful Interrupts
pub static GLOBAL_CANCEL_SIGNAL: AtomicBool = AtomicBool::new(false);
/// ⚡ Global Signal for Native In-Flight Logit Clamping (Skip Thinking)
pub static GLOBAL_SKIP_THINKING_SIGNAL: AtomicBool = AtomicBool::new(false);
