//! 📦 Cluaiz Models Subsystem (`engine/models`)
//! Contains HuggingFace downloader, installed vault registry, taxonomy, prober, and unified GGUF reader.

pub mod capabilities;
pub mod fetcher;
pub mod gguf;
pub mod prober;
pub mod registry;
pub mod taxonomy;
pub mod types;

#[cfg(test)]
mod test_fixtures;

pub use crate as models;

pub use fetcher::{
    parse_hf_repository, resolve_model_repo, resolve_model_repo_with_fallback, RegistryResolveError,
    AutoHeal, DownloadEvent, HfTreeItem, HfVariant, HuggingFaceHub,
    ModelDownloader,
};
pub use gguf::{GgufProber, ModelGeometry, WeightBytes, KV_CELL_PAD};
pub use registry::auditor::{HardwareAuditor, HealthStatus};
pub use registry::ModelCatalog as CoreRoster;
pub use types::manifest::{ModelAsset, ModelManifest, ModelRecommendation};
pub use types::{KernelSignature, StreamingReasoningFilter, StructuralDNA};
