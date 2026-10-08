//! 🖥️ Hardware Subsystem Data Types
//! Defines CPU, Memory, GPU, Storage, and System Truth structures.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub cpu: CpuSubsystem,
    pub memory: MemorySubsystem,
    pub storage: Vec<StorageSubsystem>,
    pub gpus: Vec<GpuSubsystem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuSubsystem {
    pub brand: String,
    pub physical_cores: usize,
    pub logical_threads: usize,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemorySubsystem {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub total_gb: f64,
    pub available_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageSubsystem {
    pub mount_point: String,
    pub total_gb: f64,
    pub free_gb: f64,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuSubsystem {
    pub index: u32,
    pub name: String,
    pub vendor: String,
    pub vram_total_bytes: u64,
    pub vram_free_bytes: u64,
    pub vram_total_gb: f64,
    pub vram_available_gb: f64,
    pub compute_capability: Option<String>,
    pub pcie_gen: Option<u32>,
    pub pcie_width: Option<u32>,
}
