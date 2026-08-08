//! ═══════════════════════════════════════════════════════════════════════
//!   External Crate: System Booster (Bare Metal Isolator)
//! ═══════════════════════════════════════════════════════════════════════

pub mod speculative;
pub mod dflash;
pub mod manager;
pub mod os_tuning;


pub mod telemetry;
pub mod system_optimization;

// 🏛️ Reusing the Unified Architecture from archer-shared
pub use cluaiz_shared::hardware::governor::HardwareGovernor;
pub use cluaiz_shared::hardware::schema::optimization::{OptimizationControl, FeatureState};
pub use system_optimization::*;
