//! 📦 Cluaiz Models Subsystem (`engine/models`)
//! Contains HuggingFace downloader, installed vault registry, taxonomy, prober, and unified GGUF reader.

pub mod gguf;
pub mod types;
pub mod taxonomy;
pub mod prober;
pub mod registry;
pub mod fetcher;

pub use gguf::{ModelGeometry, GgufProber};
