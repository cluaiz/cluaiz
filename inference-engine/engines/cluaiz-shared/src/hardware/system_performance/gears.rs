use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use super::pulse_schema::LivePulse;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EngineGear {
    Pulse,     // Idle/Low
    Balanced,  // Normal
    Survival,  // High pressure
    Emergency, // Critical
}

pub struct ObservableHardwareState {
    pub pulse: Arc<RwLock<LivePulse>>,
    pub turbo_quant_enabled: AtomicBool,
    pub tps_counter: std::sync::atomic::AtomicUsize,
}

impl ObservableHardwareState {
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
