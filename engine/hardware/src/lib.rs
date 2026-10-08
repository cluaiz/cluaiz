//! 🖥️ Cluaiz Hardware Discovery & Live Prober (`engine/hardware`)
//! Extracts ground truth metrics for CPU, Memory, NVML GPU, and Storage.

pub mod types;
pub mod prober;

pub use types::{SystemInfo, CpuSubsystem, MemorySubsystem, StorageSubsystem, GpuSubsystem};
pub use prober::HardwareProber;
