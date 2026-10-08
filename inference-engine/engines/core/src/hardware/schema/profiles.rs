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
pub struct SystemInfo {
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
    pub gflops_fp32: f64,
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
    pub tflops_fp32: f64,
    pub pcie_h2d_bandwidth_gbps: f64,
    pub pcie_roundtrip_latency_us: f64,
    pub pcie_link_gen: u32,
    pub pcie_link_width: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct TimingBreakdown {
    pub vram_execution: Option<GpuExecutionTiming>,
    pub pcie_streaming: Option<PcieStreamingTiming>,
    pub cpu_direct: CpuDirectTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct GpuExecutionTiming {
    pub vram_read_ms: f64,
    pub cuda_compute_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PcieStreamingTiming {
    pub pcie_transfer_ms: f64,
    pub vram_read_ms: f64,
    pub cuda_compute_ms: f64,
    pub bus_hop_penalty_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct CpuDirectTiming {
    pub ram_read_ms: f64,
    pub simd_compute_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PingPongLatency {
    pub ram_to_cpu_latency_ns: f64,
    pub ram_to_gpu_pcie_roundtrip_us: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct HardwareStateSnapshot {
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub cpu_utilization_pct: f32,
    pub cpu_clock_ghz: f64,
    pub cpu_peak_gflops: f64,
    pub ram_total_gb: f64,
    pub ram_used_gb: f64,
    pub ram_free_gb: f64,
    pub ram_usable_after_buffer_gb: f64,
    pub ram_read_bandwidth_gbps: f64,
    pub ram_latency_ns: f64,
    pub gpu_model: String,
    pub gpu_total_vram_gb: f64,
    pub gpu_used_vram_gb: f64,
    pub gpu_free_vram_gb: f64,
    pub gpu_usable_after_buffer_gb: f64,
    pub gpu_vram_bandwidth_gbps: f64,
    pub gpu_pcie_bandwidth_gbps: f64,
    pub gpu_pcie_roundtrip_us: f64,
    pub gpu_peak_tflops: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ExecutionCandidate {
    pub id: String,
    pub name: String,
    pub feasible: bool,
    pub ineligibility_reason: Option<String>,
    pub gpu_layers: usize,
    pub cpu_layers: usize,
    pub total_layers: usize,
    pub vram_required_gb: f64,
    pub ram_required_gb: f64,
    pub kv_location: String,
    pub kv_cache_gb: f64,
    pub prompt_ms_per_token: f64,
    pub generation_ms_per_token: f64,
    pub generation_tps: f64,
    pub ttft_ms: f64,
    pub is_winner: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ModelCapabilities {
    pub supports_images: bool,
    pub supported_inputs: Vec<String>,
    pub supported_attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ModelPlanInfo {
    pub model_id: String,
    pub architecture: String,
    pub model_size_gb: f64,
    pub total_layers: usize,
    pub attention_heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub max_input_context: usize,
    pub max_output_context: usize,
    pub active_working_context: usize,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub supported_attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MemoryBreakdown {
    pub model_in_vram_gb: f64,
    pub model_in_ram_gb: f64,
    pub kv_cache_memory_gb: f64,
    pub remaining_free_vram_gb: f64,
    pub remaining_free_ram_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ComputeCapabilityRates {
    pub gpu_speed_ms_per_layer: f64,
    pub cpu_speed_ms_per_layer: f64,
    pub gpu_to_cpu_power_ratio: f64,
    pub compute_winner: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct BusTimings {
    pub total_time_per_token_ms: f64,
    pub gpu_execution_time_ms: f64,
    pub ram_to_cpu_execution_time_ms: f64,
    pub ram_to_gpu_transfer_time_ms: f64,
    pub ram_to_gpu_sync_delay_us: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ExecutionStrategyPlan {
    pub placement_mode: String,
    pub gpu_layers: usize,
    pub ram_layers: usize,
    pub ssd_layers: usize,
    pub kv_cache_location: String,
    pub ram_layers_compute_target: String,
    pub model_memory_breakdown: MemoryBreakdown,
    pub compute_capability_rates: ComputeCapabilityRates,
    pub bus_timings: BusTimings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct GpuHardwareInfo {
    pub model: String,
    pub total_vram_gb: f64,
    pub free_vram_gb: f64,
    pub safety_buffer_gb: f64,
    pub usable_vram_gb: f64,
    pub vram_speed_gbps: f64,
    pub compute_capability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct CpuHardwareInfo {
    pub model: String,
    pub cores: usize,
    pub threads: usize,
    pub clock_ghz: f64,
    pub peak_gflops: f64,
    pub current_load_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct RamHardwareInfo {
    pub total_ram_gb: f64,
    pub free_ram_gb: f64,
    pub safety_buffer_gb: f64,
    pub usable_ram_gb: f64,
    pub read_speed_gbps: f64,
    pub access_latency_ns: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct WorkspaceBufferInfo {
    pub compute_workspace_gb: f64,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct StorageSsdInfo {
    pub drive_type: String,
    pub speed_mbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PcieBusInfo {
    pub bus_type: String,
    pub transfer_speed_gbps: f64,
    pub bus_latency_us: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct HardwareSnapshotInfo {
    pub gpu: GpuHardwareInfo,
    pub cpu: CpuHardwareInfo,
    pub ram: RamHardwareInfo,
    pub workspace_buffer: WorkspaceBufferInfo,
    pub storage_ssd: StorageSsdInfo,
    pub gpu_pcie_bus: PcieBusInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct ActiveUserSettings {
    pub target_slot: Option<String>,
    pub n_gpu_layers: i32,
    pub n_ctx: usize,
    pub batch_size: usize,
    pub ubatch_size: usize,
    pub flash_attention: String,
    pub kv_cache_quantization: String,
    pub hybrid_memory: String,
    pub vram_buffer_gb: f64,
    pub ram_buffer_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct HardwareExecutionComparison {
    pub active_user_settings: ActiveUserSettings,
    pub model_info: ModelPlanInfo,
    pub execution_plan: ExecutionStrategyPlan,
    pub hardware_info: HardwareSnapshotInfo,
    pub model_name: Option<String>,
    pub architecture: Option<String>,
    pub weights_gb: f64,
    pub batch_tokens: usize,
    pub total_layers: usize,
    pub n_ctx: usize,
    pub max_input_tokens: usize,
    pub max_output_tokens: usize,
    pub supported_inputs: Vec<String>,
    pub supported_outputs: Vec<String>,
    pub supported_attachments: Vec<String>,
    pub supported_modalities: Vec<String>,
    pub kv_cache_gb: f64,
    pub compute_buffer_gb: f64,
    pub model_path: Option<String>,
    pub vram_safety_buffer_gb: f64,
    pub ram_safety_buffer_gb: f64,
    pub vram_free_needed_gb: f64,
    pub ram_free_needed_gb: f64,
    pub gpu_layers: usize,
    pub cpu_layers: usize,
    pub model_fits_in_vram: bool,
    pub faster_target: String,
    pub speedup_ratio: f64,
    pub cpu_latency_ms: f64,
    pub gpu_pcie_latency_ms: f64,
    pub gpu_vram_latency_ms: Option<f64>,
    pub decision_reason: String,
    pub breakdown: String,
    pub timing_breakdown: TimingBreakdown,
    pub bus_ping_pong_latency: PingPongLatency,
    pub hardware_state: HardwareStateSnapshot,
    pub candidates: Vec<ExecutionCandidate>,
    pub winner_candidate_id: String,
    pub winner_generation_tps: f64,
    pub winner_ttft_ms: f64,
}


