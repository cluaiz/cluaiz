//! ⚡ Live Hardware Telemetry & Pulse Observer (`engine/hardware/src/telemetry.rs`)
//! Continuous background thread polling OS CPU/RAM and NVML GPU status.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;
use sysinfo::System;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EngineGear {
    Pulse,     // Idle/Low
    Balanced,  // Normal
    Survival,  // High pressure
    Emergency, // Critical
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LivePulse {
    pub timestamp: u64,
    pub cpu: CpuPulse,
    pub ram: RamPulse,
    pub gpus: Vec<GpuPulse>,
    pub npus: Vec<AcceleratorPulse>,
    pub tpus: Vec<AcceleratorPulse>,
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
    pub name: String,
    pub utilization_pct: f32,
    pub temperature_c: f32,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,
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

pub struct ObservableHardwareState {
    pub pulse: Arc<RwLock<LivePulse>>,
    pub turbo_quant_enabled: AtomicBool,
    pub tps_counter: AtomicUsize,
}

impl ObservableHardwareState {
    pub fn new() -> Self {
        Self {
            pulse: Arc::new(RwLock::new(LivePulse::default())),
            turbo_quant_enabled: AtomicBool::new(false),
            tps_counter: AtomicUsize::new(0),
        }
    }

    pub fn resolve_gear(&self) -> EngineGear {
        let p = self.pulse.read().unwrap_or_else(|e| e.into_inner());
        let primary_gpu_util = p.gpus.first().map(|g| g.utilization_pct).unwrap_or(0.0);

        if p.cpu.utilization_pct > 95.0 || primary_gpu_util > 95.0 {
            EngineGear::Emergency
        } else if p.cpu.utilization_pct > 80.0 || primary_gpu_util > 80.0 {
            EngineGear::Survival
        } else if p.cpu.utilization_pct > 40.0 {
            EngineGear::Balanced
        } else {
            EngineGear::Pulse
        }
    }
}

static GLOBAL_TELEMETRY: Lazy<Arc<ObservableHardwareState>> = Lazy::new(|| {
    let state = Arc::new(ObservableHardwareState::new());
    let state_clone = state.clone();

    std::thread::Builder::new()
        .name("cluaiz-hw-telemetry".to_string())
        .spawn(move || {
            let mut sys = System::new_all();
            loop {
                sys.refresh_cpu_usage();
                sys.refresh_memory();

                let cpu_usage = sys.global_cpu_info().cpu_usage();
                let total_mem = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
                let used_mem = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
                let ram_pct = if total_mem > 0.0 { (used_mem / total_mem * 100.0) as f32 } else { 0.0 };

                let mut gpus = Vec::new();
                #[cfg(not(target_os = "macos"))]
                {
                    if let Ok(nvml) = nvml_wrapper::Nvml::init() {
                        if let Ok(device) = nvml.device_by_index(0) {
                            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
                            let util = device.utilization_rates().map(|u| u.gpu as f32).unwrap_or(0.0);
                            let temp = device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu).map(|t| t as f32).unwrap_or(0.0);
                            let mem_info = device.memory_info().ok();
                            let vram_total_gb = mem_info.as_ref().map(|m| m.total as f64 / 1024.0 / 1024.0 / 1024.0).unwrap_or(0.0);
                            let vram_used_gb = mem_info.as_ref().map(|m| m.used as f64 / 1024.0 / 1024.0 / 1024.0).unwrap_or(0.0);
                            let power = device.power_usage().map(|p| p as f32 / 1000.0).unwrap_or(0.0);

                            gpus.push(GpuPulse {
                                name,
                                utilization_pct: util,
                                temperature_c: temp,
                                vram_used_gb,
                                vram_total_gb,
                                power_draw_watts: power,
                                fan_speed_pct: 0,
                            });
                        }
                    }
                }

                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                if let Ok(mut pulse) = state_clone.pulse.write() {
                    pulse.timestamp = timestamp;
                    pulse.cpu = CpuPulse {
                        utilization_pct: cpu_usage,
                        temperature_c: 0.0,
                        clock_ghz: 0.0,
                    };
                    pulse.ram = RamPulse {
                        used_gb: used_mem,
                        utilization_pct: ram_pct,
                    };
                    pulse.gpus = gpus;
                }

                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        })
        .ok();

    state
});

pub fn get_pulse() -> Arc<ObservableHardwareState> {
    GLOBAL_TELEMETRY.clone()
}
