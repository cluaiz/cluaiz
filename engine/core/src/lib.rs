//! 🧠 Cluaiz Core Engine Library (`engine/core`)
//! Provides centralized environment resolution, units, contracts, and cached settings.

pub mod env;
pub mod settings;
pub mod units;
pub mod contracts;
pub mod chat;
pub mod tool_calls;
pub mod prompts;
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
    ChatEnvelope, ChatMessage, OutputConstraint, EARLIER_RESULT_NOTE, STEP_RESULT_NOTE, answer_reserve, oldest_earlier_result, SAME_RESULT_NOTE, shorten_a_step_result, oldest_steps_to_drop, oldest_turns_to_drop, room_to_keep, shift_discard, tokens_to_free,
};
pub use contracts::{UnifiedBackend, StreamingInference, StreamCallback, Modality, EngineError, EmbeddingDriver};

use std::sync::atomic::AtomicBool;

/// The Engine's version: `[workspace.package] version` in the root Cargo.toml, or the release's own
/// (`CLUAIZ_RELEASE_VERSION`, set by the release workflow, which checks they match). See build.rs
pub const VERSION: &str = env!("CLUAIZ_VERSION");
/// The release channel: `[workspace.metadata.cluaiz] channel` in the root Cargo.toml, such as
/// "pre-release" or "stable"
pub const CHANNEL: &str = env!("CLUAIZ_CHANNEL");
/// The version as people see it: "0.1.0 (pre-release)", or only "0.1.0" for a stable release
pub const VERSION_SHOWN: &str = env!("CLUAIZ_VERSION_SHOWN");

/// 🛑 Global Cancellation Signal for Graceful Interrupts
pub static GLOBAL_CANCEL_SIGNAL: AtomicBool = AtomicBool::new(false);
