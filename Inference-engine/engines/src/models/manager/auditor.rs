use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use cluaiz_shared::hardware::schema::profiles::SystemControl;

/// Hardware Health Auditor
/// Enforces the 5/10 TPS Rule and VRAM safety.
pub struct HardwareAuditor;

impl HardwareAuditor {
    /// Audits hardware performance for a model based on manifest specs
    pub fn audit_performance(&self, ram_required: f32, requires_gpu: bool) -> HealthStatus {
        let config_path = self.get_system_control_path();
        
        let system_control = if let Ok(content) = std::fs::read_to_string(config_path) {
            serde_json::from_str::<SystemControl>(&content).ok()
        } else {
            None
        };

        match system_control {
            Some(control) => self.evaluate_hardware(&control, ram_required, requires_gpu),
            None => HealthStatus::Suboptimal, // Fallback
        }
    }

    fn evaluate_hardware(&self, control: &SystemControl, req_ram: f32, req_gpu: bool) -> HealthStatus {
        // 🚀 cluaiz Logic: Extract VRAM from the first GPU (Primary)
        let vram_available = control.silicon_truth.accelerators.gpus.first()
            .map(|g| g.vram_available_gb)
            .unwrap_or(0.0) as f32;
        
        let system_ram = control.silicon_truth.memory.total_capacity_gb as f32;

        if req_gpu {
            if req_ram <= vram_available {
                HealthStatus::Healthy    // Green (> 10 TPS)
            } else if req_ram <= system_ram {
                HealthStatus::Suboptimal // Yellow (5-10 TPS)
            } else {
                HealthStatus::Disabled   // Black (Not enough resources)
            }
        } else {
            // CPU Inference Logic
            if req_ram <= system_ram * 0.4 {
                HealthStatus::Suboptimal // Yellow
            } else if req_ram <= system_ram {
                HealthStatus::Heavy      // Red (< 5 TPS)
            } else {
                HealthStatus::Disabled   // Black
            }
        }
    }

    fn get_system_control_path(&self) -> PathBuf {
        cluaiz_shared::hardware::governor::HardwareGovernor::resolve_engine_path().join("system_control.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,    // Green (Optimal)
    Suboptimal, // Yellow (Average)
    Heavy,      // Red (Heavy)
    Cloud,      // Blue (Cloud API)
    Disabled,   // Black (Disabled)
}
