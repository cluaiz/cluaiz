//! 💾 Expert Offloading Subsystem (Colibri MoE Architecture — Adapted for GGUF + ONNX)
//! Enables high-capacity MoE model execution on low-RAM hardware by streaming
//! expert weights on demand from NVMe storage into an LRU RAM pool.
//!
//! Components:
//! - `moe_detector`  : Reads GGUF/ONNX headers to detect MoE architecture before loading
//! - `expert_index`  : Maps (layer, expert_id) → file byte offsets for targeted reads
//! - `routing_heat`  : Tracks hot experts across sessions (.cluaiz_routing_heat persistence)
//! - `expert_cache`  : LRU RAM pool for active expert weight blocks
//! - `mmap_streamer` : OS-level memory-mapped reads for expert tensor loading

pub mod expert_cache;
pub mod expert_index;
pub mod mmap_streamer;
pub mod moe_detector;
pub mod routing_heat;

pub use expert_cache::{ExpertCacheManager, ExpertKey, LoadedExpertBlock, SharedExpertCache};
pub use expert_index::{ExpertOffsetIndex, ExpertTensorOffset, TensorRange};
pub use mmap_streamer::SsdMmapStreamer;
pub use moe_detector::{detect_moe, GgufMoeDetector, MoeModelInfo, OnnxMoeDetector};
pub use routing_heat::RoutingHeatTracker;

