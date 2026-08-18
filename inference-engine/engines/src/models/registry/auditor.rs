//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Hardware Health Auditor (RAM / VRAM Safety)
//! ═══════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use cluaiz_shared::hardware::schema::profiles::SystemControl;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,    // Green (Optimal > 10 TPS)
    Suboptimal, // Yellow (Average 5-10 TPS)
    Heavy,      // Red (Heavy < 5 TPS)
    Cloud,      // Blue (Cloud API)
    Disabled,   // Black (Not enough memory)
}

pub struct HardwareAuditor;

impl HardwareAuditor {
    pub fn audit_performance(&self, ram_required: f32, requires_gpu: bool) -> HealthStatus {
        let config_path = self.get_system_control_path();
        
        let system_control = if let Ok(content) = std::fs::read_to_string(config_path) {
            serde_json::from_str::<SystemControl>(&content).ok()
        } else {
            None
        };

        match system_control {
            Some(control) => self.evaluate_hardware(&control, ram_required, requires_gpu),
            None => HealthStatus::Suboptimal,
        }
    }

    fn evaluate_hardware(&self, control: &SystemControl, req_ram: f32, req_gpu: bool) -> HealthStatus {
        let vram_available = control.silicon_truth.accelerators.gpus.first()
            .map(|g| g.vram_available_gb)
            .unwrap_or(0.0) as f32;
        
        let system_ram = control.silicon_truth.memory.total_capacity_gb as f32;

        if req_gpu {
            if req_ram <= vram_available {
                HealthStatus::Healthy
            } else if req_ram <= system_ram {
                HealthStatus::Suboptimal
            } else {
                HealthStatus::Disabled
            }
        } else if req_ram <= system_ram * 0.4 {
            HealthStatus::Suboptimal
        } else if req_ram <= system_ram {
            HealthStatus::Heavy
        } else {
            HealthStatus::Disabled
        }
    }

    fn get_system_control_path(&self) -> PathBuf {
        cluaiz_shared::hardware::governor::HardwareGovernor::resolve_engine_path().join("system_control.json")
    }
}
