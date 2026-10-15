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
    resolve_model_repo, AutoHeal, DownloadEvent, HfTreeItem, HfVariant, HuggingFaceHub,
    ModelDownloader,
};
pub use gguf::{GgufProber, ModelGeometry, WeightBytes, KV_CELL_PAD};
pub use registry::auditor::{HardwareAuditor, HealthStatus};
pub use registry::ModelCatalog as CoreRoster;
pub use types::manifest::{ModelAsset, ModelManifest, ModelRecommendation};
pub use types::{KernelSignature, StreamingReasoningFilter, StructuralDNA};
