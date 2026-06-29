use std::sync::Arc;
use cluaiz_shared::hardware::system_performance::{ObservableHardwareState, get_pulse};
use tracing::info;

#[derive(Debug, Clone, PartialEq)]
pub enum HardwareTarget {
    CPU,
    GPU,
}

/// 🧠 The Telemetry Bridge
/// Links the live hardware sensors (system-booster) to the Neural Core.
/// Makes dynamic decisions on where to run models to prevent OOM (Out-of-Memory) crashes.
pub struct TelemetryBridge {
    pub state: Arc<ObservableHardwareState>,
}

impl Default for TelemetryBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryBridge {
    pub fn new() -> Self {
        Self {
            state: get_pulse(),
        }
    }

    /// Calculates the optimal execution target based on real-time VRAM pressure.
    pub fn get_optimal_target(&self, required_vram_gb: f64) -> HardwareTarget {
        if let Ok(pulse) = self.state.pulse.read() {
            let free_vram = pulse.vram_total_gb - pulse.vram_used_gb;
            
            // If we have enough free VRAM and are not redlining the GPU
            if free_vram > required_vram_gb && pulse.vram_pressure_pct < 95 {
                info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB). Routing execution to GPU.", free_vram);
                return HardwareTarget::GPU;
            } else {
                info!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB). Auto-falling back to CPU AVX.", free_vram);
                return HardwareTarget::CPU;
            }
        }
        
        info!("📡 [Telemetry] Sensor lock failed. Defaulting to safe CPU fallback.");
        HardwareTarget::CPU
    }
}
