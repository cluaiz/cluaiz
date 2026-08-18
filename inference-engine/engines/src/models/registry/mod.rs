//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: The Core Roster, Installed State & Model Vault Subsystem
//! ═══════════════════════════════════════════════════════════════════════

pub mod vault;
pub mod installed_state;
pub mod catalog;
pub mod discovery;
pub mod auditor;
pub mod provisioner;

pub use vault::{CategoryDescriptor, ModelVault};
pub use installed_state::InstalledStateRegistry;
pub use catalog::{ModelCatalog, REGISTRY_URL};
pub use discovery::AutonomousDiscovery;
pub use auditor::{HardwareAuditor, HealthStatus};
pub use provisioner::Provisioner;
pub use cluaiz_shared::{KernelSignature, StructuralDNA};
pub use crate::models::types::{ModelAsset, ModelManifest, ModelRecommendation};

/// Legacy alias for ModelCatalog
pub type CoreRoster = ModelCatalog;
