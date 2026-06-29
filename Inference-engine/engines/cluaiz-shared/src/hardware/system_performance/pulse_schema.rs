use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LivePulse {
    pub timestamp: u64,
    pub cpu: CpuPulse,
    pub ram: RamPulse,
    pub gpus: Vec<GpuPulse>,           // 🚀 MULTI-GPU SUPPORT
    pub npus: Vec<AcceleratorPulse>,
    pub tpus: Vec<AcceleratorPulse>,

    // 🏛️ Engine-wide Pressure Metrics
    pub vram_pressure_pct: u32,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,
    pub relay_latency_ms: u64,
    pub kv_cache_footprint_mb: u64,
    pub storage_throughput_mbps: u64,
    pub network_throughput_mbps: u64,
    pub per_core_usage: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CpuPulse {
    pub utilization_pct: f32,
    pub temperature_c: f32,
    pub clock_ghz: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RamPulse {
    pub used_gb: f64,
    pub utilization_pct: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GpuPulse {
    pub name: String,                  // 🚀 Identifying the silicon
    pub utilization_pct: f32,
    pub temperature_c: f32,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,            // 🚀 Added for pressure calc
    pub power_draw_watts: f32,
    pub fan_speed_pct: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AcceleratorPulse {
    pub name: String,
    pub utilization_pct: f32,
    pub temperature_c: f32,
    pub power_draw_watts: f32,
}
