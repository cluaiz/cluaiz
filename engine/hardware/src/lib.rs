//! 🖥️ Cluaiz Hardware Discovery & Live Prober (`engine/hardware`)
//! Extracts ground truth metrics for CPU, Memory, NVML GPU, and Storage.

pub mod types;
pub mod prober;
pub mod telemetry;

pub use types::{SystemInfo, CpuSubsystem, MemorySubsystem, StorageSubsystem, GpuSubsystem};
pub use prober::HardwareProber;
pub use telemetry::{ObservableHardwareState, LivePulse, CpuPulse, RamPulse, GpuPulse, EngineGear, get_pulse};

