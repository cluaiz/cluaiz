#![allow(warnings)]
//! ═══════════════════════════════════════════════════════════════════════
//!   Engine: Master Archer Unified Core Gateway
//! ═══════════════════════════════════════════════════════════════════════

// 🧬 Archer cluaiz Core Core

pub mod api;
// pub mod kernel; // Removed in V7 DNA Refactor
pub mod runtime;
pub mod models;
pub mod memory;
pub mod hardware;
pub mod telemetry;
pub mod utils;
pub mod cli;
pub mod sync;
#[path = "interface-engines/mod.rs"]
pub mod interface_engines;
pub mod neural_foundry;
#[cfg(feature = "system-booster")]
pub use system_booster;

// ─── Master Archer Unified Access ────────────────────────────────────

// pub use kernel::core::CureKernel; // Removed in V7 DNA Refactor

// 2. Hardware & Architecture
pub use hardware::{HardwareDetector, SiliconTruth, SiliconTruth as HardwareInfo, InferenceEngine, InferenceEvent};
pub use hardware::system_control_manager::{detect_hardware, has_config, read_config, save_config, update_field};

// 3. Execution & Inference
pub use runtime::execution::runner::{cluaizRunner, cluaizMetrics};
pub use runtime::execution::sampler::CoreSampler;
pub use runtime::execution::loader::GGUFLoader;

// 4. Intelligence & Registry
pub use models::entities::{ChatMessage, ChatRequest, ChatResponse, ChatSession, MessageRole};
pub use models::registry::{CoreRoster, ModelManifest, ModelRecommendation, ModelAsset};
pub use models::fetch::{DownloadEvent, ModelDownloader};

// 5. Routing
pub use api::router::CoreRouter;
pub use cluaiz_shared::BackendType;
