//! 📦 Cluaiz Models Subsystem (`engine/models`)
//! Contains HuggingFace downloader, installed vault registry, taxonomy, prober, and unified GGUF reader.

pub mod gguf;
pub mod types;
pub mod taxonomy;
pub mod prober;
pub mod registry;
pub mod fetcher;
pub mod ingest;

pub use crate as models;

pub use gguf::{ModelGeometry, WeightBytes, GgufProber, KV_CELL_PAD};
pub use types::{StructuralDNA, KernelSignature, StreamingReasoningFilter};
pub use types::manifest::{ModelManifest, ModelRecommendation, ModelAsset};
pub use fetcher::{DownloadEvent, ModelDownloader, HuggingFaceHub, HfVariant, HfTreeItem, AutoHeal, resolve_model_repo};
pub use registry::ModelCatalog as CoreRoster;
pub use registry::auditor::{HardwareAuditor, HealthStatus};
pub use ingest::{DocumentIngestor, SemanticChunker};

