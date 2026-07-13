//! 📦 Tier 7: Schema - Sovereign Profiles
//! Static hardware definitions and profiles. No logic allowed.

use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SovereignIdentity {
    pub machine_name: String,
    pub os_target: String,
    pub architecture: String,
    pub kernel_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SovereignContext {
    pub cluaiz_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SovereignBrain {
    // DB is now loaded as a plugin, hardcoded references removed.
}

impl SovereignBrain {
    pub fn is_enabled(&self) -> bool {
        false
    }

    pub fn is_pure_brain(&self) -> bool {
        false
    }

    pub fn is_local(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SystemControl {
    pub identity: SovereignIdentity,
    pub context: SovereignContext,
    pub brain: SovereignBrain,
    pub silicon_truth: SiliconTruth,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SiliconTruth {
    pub compute_architecture_type: Option<String>,
    pub cpu: CpuSubsystem,
    pub memory: MemorySubsystem,
    pub storage: Vec<StorageSubsystem>,
    pub accelerators: Accelerators,
    pub active_drivers: Vec<EngineDriver>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct CpuSubsystem {
    pub brand: String,
    pub architecture: String,
    pub numa_nodes: u32,
    pub base_clock_mhz: f64,
    pub boost_clock_mhz: f64,
    pub physical_cores: u32,
    pub logical_threads: u32,
    pub l1_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,
    pub isa_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub enum HardwareVendor {
    NVIDIA,
    AMD,
    Intel,
    Apple,
    Qualcomm,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Default)]
#[archive(check_bytes)]
pub enum BackendDriver {
    #[default]
    CPU,
    CUDA,
    METAL,
    ROCM,
    SYCL,
    OpenVINO,
    Vulkan,
    DirectML,
    OpenCL,
    NNAPI,
    Hexagon,
    QNN,
    TPU,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MemorySubsystem {
    pub total_capacity_gb: f64,
    pub available_capacity_gb: f64,
    pub type_name: String,
    pub speed_mts: f64,
    pub bandwidth_gbps: f64,
    pub memory_latency_ns: f64,
    pub is_unified_memory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct DFlashMetadata {
    pub nand_type: String,
    pub controller: String,
    pub wear_level_percent: f64,
    pub total_host_writes_tb: f64,
    pub health_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct StorageSubsystem {
    pub mount_point: String,
    pub drive_type: String,
    pub bus: String,
    pub read_speed_mbps: f64,
    pub write_speed_mbps: f64,
    pub total_gb: f64,
    pub free_gb: f64,
    pub is_primary_workspace: bool,
    pub dflash: Option<DFlashMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct Accelerators {
    pub gpus: Vec<GpuSubsystem>,
    pub npus: Vec<NpuSubsystem>,
    pub tpus: Vec<TpuSubsystem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct GpuSubsystem {
    pub vendor: String,
    pub model: String,
    pub vram_total_gb: f64,
    pub vram_reserved_by_os_mb: f64,
    pub vram_available_gb: f64,
    pub vram_type: String,
    pub bandwidth_gbps: f64,
    pub bus_width_bit: u32,
    pub compute_cores: u32,
    pub compute_capability: Option<String>,
    pub l2_cache_mb: f64,
    pub max_tdp_watts: f64,
    pub current_thermal_limit_c: f64,
    pub connection: String,
    pub is_unified_with_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct NpuSubsystem {
    pub vendor: String,
    pub model: String,
    pub tops: f64,
    pub precision_support: Vec<String>,
    pub driver_interface: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct TpuSubsystem {
    pub vendor: String,
    pub model: String,
    pub tpu_type: String,
    pub tops: f64,
    pub interface: String,
    pub supported_precision: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct SovereignProfile {
    pub platform: String,
    pub compute_architecture_type: Option<String>,
    pub cpu_brand: String,
    pub cpu_cores: u32,
    pub cpu: CpuSubsystem,
    pub accelerators: Accelerators,
    pub compute: LegacyCompute,
    pub active_drivers: Vec<EngineDriver>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct LegacyCompute {
    pub has_gpu: bool,
    pub vram_gb: f64,
    pub primary_driver: BackendDriver,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct EngineDriver {
    pub driver_id: String,
    pub status: String,
    pub version: Option<String>,
}

/// 🧬 cluaiz Bridge Alias: cluaizProfile is the canonical name for SovereignProfile.
/// Kept for backward compat during naming migration.
pub type cluaizProfile = SovereignProfile;
