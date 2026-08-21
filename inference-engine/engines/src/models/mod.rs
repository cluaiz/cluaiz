//! ═══════════════════════════════════════════════════════════════════════
//!   Models: Cluaiz Unified Sovereign Model Subsystem (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

pub mod types;
pub mod taxonomy;
pub mod prober;
pub mod fetcher;
pub mod registry;
pub mod manager;

pub use fetcher as fetch;

pub use types::*;
pub use taxonomy::{
    ClassificationResult, ModelCapabilities, SttFamily, SttTaxonomy, TtsFamily, TtsTaxonomy,
    UniversalModelClassifier, UniversalQuantization, UniversalTaskRules,
};
pub use prober::{GgufProbeResult, GgufProber, ModelProber, OnnxProbeResult, OnnxProber};
pub use fetcher::{
    AssetBundler, AutoHeal, DownloadEvent, FileDownloader, HfTreeItem, HfVariant, HuggingFaceHub,
    ModelDownloader, RegistryClient,
};
pub use registry::{
    AutonomousDiscovery, CategoryDescriptor, CoreRoster, HardwareAuditor, HealthStatus,
    InstalledStateRegistry, ModelCatalog, ModelVault, Provisioner,
};
pub use manager::ModelManager;
