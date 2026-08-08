//! 💾 SSD Streaming Subsystem (Colibri MoE Zero-OOM Architecture)
//! Enables high-capacity MoE model execution on low-RAM hardware by streaming
//! expert weights on demand directly from high-speed NVMe SSDs into an LRU RAM pool.

pub mod expert_cache;
pub mod mmap_streamer;

pub use expert_cache::{ExpertCacheManager, LoadedExpertBlock, SharedExpertCache};
pub use mmap_streamer::SsdMmapStreamer;
