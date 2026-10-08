//! 🧠 Cluaiz Core Engine Library (`engine/core`)
//! Provides centralized environment resolution, units, contracts, and cached settings.

pub mod env;
pub mod settings;
pub mod units;
pub mod contracts;

pub use env::{EnvironmentManager, EnvironmentMode};
pub use settings::{Settings, ActiveUserSettings, OptimizationControl, FeatureState, KvCacheQuantization, ContextShiftingMode};
pub use units::{BYTE, KIB, MIB, GIB, bytes_to_gib, bytes_to_mib, gib_to_bytes, mib_to_bytes};
pub use contracts::{UnifiedBackend, StreamingInference, StreamCallback};
