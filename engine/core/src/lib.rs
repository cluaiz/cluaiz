//! 🧠 Cluaiz Core Engine Library (`engine/core`)
//! Provides centralized environment resolution, units, contracts, and cached settings.

pub mod env;
pub mod settings;
pub mod units;
pub mod contracts;
pub mod chat;
pub mod tool_calls;
pub mod live;
pub mod media;
pub mod media_limits;
pub mod processing;
pub mod model_assets;

pub use env::{EnvironmentManager, EnvironmentMode};
pub use settings::{
    Settings, ActiveUserSettings, ChatMemorySettings, OptimizationControl, FeatureState, KvCacheQuantization, ContextShiftingMode,
    ContextSetting, SamplerDefaults, LlamaLoadParams, ThinkingBudget, ThinkingLimit, ThinkingPresets, ThinkingShare, OnnxSettings, set_active_context, active_context_tokens, active_model_path,
    DEFAULT_SAFE_VRAM_BUFFER_GB, MIN_SAFE_VRAM_BUFFER_GB, DEFAULT_SAFE_RAM_BUFFER_GB, MIN_SAFE_RAM_BUFFER_GB,
};
pub use units::{BYTE, KIB, MIB, GIB, GIB_F64, MIB_F64, bytes_to_gib, bytes_to_mib, gib_to_bytes, mib_to_bytes};
pub use chat::{
    ChatEnvelope, ChatMessage, OutputConstraint, answer_reserve, oldest_turns_to_drop, shift_discard, tokens_to_free,
};
pub use contracts::{UnifiedBackend, StreamingInference, StreamCallback, Modality, EngineError, EmbeddingDriver};

use std::sync::atomic::AtomicBool;

/// The Engine's version: the release's own (`CLUAIZ_RELEASE_VERSION`, set by the release workflow,
/// whose tag can be newer than Cargo.toml), else the crate version of a local build
pub const VERSION: &str = match option_env!("CLUAIZ_RELEASE_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};

/// 🛑 Global Cancellation Signal for Graceful Interrupts
pub static GLOBAL_CANCEL_SIGNAL: AtomicBool = AtomicBool::new(false);
