//! 🖥️ Hardware Subsystem Data Types
//! Defines CPU, Memory, GPU, Storage, and System Truth structures.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub architecture: String,
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
    pub base_clock_mhz: f64,
    pub boost_clock_mhz: f64,
    pub l1_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,
    pub isa_features: Vec<String>,
    pub gflops_fp32: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemorySubsystem {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub total_gb: f64,
    pub available_gb: f64,
    pub type_name: String,
    pub speed_mts: f64,
    pub bandwidth_gbps: f64,
    pub memory_latency_ns: f64,
    pub is_unified_memory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageSubsystem {
    pub mount_point: String,
    pub total_gb: f64,
    pub free_gb: f64,
    pub is_primary: bool,
    pub drive_type: String,
    pub read_speed_mbps: f64,
    pub write_speed_mbps: f64,
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
    pub pcie_bandwidth_gbps: f64,
}

impl GpuSubsystem {
    /// Estimates physical onboard VRAM memory bus bandwidth (GB/s) based on capacity and architectural bus tier
    pub fn estimated_vram_bandwidth_gbps(&self) -> f64 {
        if self.vram_total_gb >= 24.0 {
            1008.0 // 384-bit GDDR6X flagship (RTX 4090 / 3090 / A5000 class)
        } else if self.vram_total_gb >= 16.0 {
            716.0 // 256-bit GDDR6X high-tier (RTX 4080 class)
        } else if self.vram_total_gb >= 12.0 {
            504.0 // 192-bit GDDR6X mid-high tier (RTX 4070 / 3080 class)
        } else if self.vram_total_gb >= 8.0 {
            288.0 // 128-bit / 192-bit GDDR6 mainstream (RTX 4060 / 3060 class)
        } else {
            192.0 // Entry / mobile GPU bus
        }
    }
}
