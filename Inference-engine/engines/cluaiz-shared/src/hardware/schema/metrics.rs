//! 📦 Tier 7: Schema - Silicon Metrics & Telemetry
//! Real-time hardware state snapshots. No logic allowed.

use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SiliconMetrics {
    pub vram_pressure: u32,
    pub cpu_thermal: i32,
    pub core_load_avg: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MemorySnapshot {
    pub system_total_gb: f64,
    pub system_used_gb: f64,
    pub system_free_gb: f64,
    pub swap_total_gb: f64,
    pub swap_used_gb: f64,
    pub is_unified: bool,
    pub shared_gpu_reserved_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MobileTelemetry {
    pub battery_level: u32,
    pub is_charging: bool,
    pub thermal_throttling: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct NPUData {
    pub brand: String,
    pub active_state: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct TPUData {
    pub brand: String,
    pub active_state: bool,
}
