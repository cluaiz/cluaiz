//! 🏛️ Sovereign Silicon Kernel: The Architectural Heart
//! Unified single-file orchestration for hardware Agnosticism.

pub mod memory;
pub mod schema;
pub mod system_control;
pub mod lookup;
pub mod system_performance;
pub mod governor;
pub mod speed_checker;
pub mod circuit_breaker;

// ── Re-exports for clean API ──
pub use system_control::HardwareOrchestrator;
pub use system_performance::pulse_schema::LivePulse;
pub use system_performance::get_pulse;
pub use governor::HardwareGovernor;

/// 📡 Helper: Quick access to the Sovereign Silicon Truth.
pub fn get_silicon_state() -> schema::profiles::SiliconTruth {
    system_control::HardwareOrchestrator::start()
        .map(|sc| sc.silicon_truth)
        .unwrap_or_default()
}

pub fn get_sovereign_profile() -> schema::profiles::SovereignProfile {
    let truth = get_silicon_state();
    schema::profiles::SovereignProfile {
        platform: truth.compute_architecture_type.clone().unwrap_or_default(),
        compute_architecture_type: truth.compute_architecture_type.clone(),
        cpu_brand: truth.cpu.brand.clone(),
        cpu_cores: truth.cpu.physical_cores,
        cpu: truth.cpu.clone(),
        accelerators: truth.accelerators.clone(),
        active_drivers: truth.active_drivers.clone(),
        compute: schema::profiles::LegacyCompute {
            has_gpu: !truth.accelerators.gpus.is_empty(),
            vram_gb: if !truth.accelerators.gpus.is_empty() { truth.accelerators.gpus[0].vram_total_gb } else { 0.0 },
            primary_driver: if !truth.accelerators.gpus.is_empty() { schema::profiles::BackendDriver::CUDA } else { schema::profiles::BackendDriver::CPU },
        }
    }
}

impl schema::profiles::SovereignProfile {
    pub fn to_silicon_truth(&self) -> schema::profiles::SiliconTruth {
        schema::profiles::SiliconTruth {
            compute_architecture_type: self.compute_architecture_type.clone(),
            cpu: self.cpu.clone(),
            memory: schema::profiles::MemorySubsystem::default(),
            storage: Vec::new(),
            accelerators: self.accelerators.clone(),
            active_drivers: self.active_drivers.clone(),
        }
    }
}

// ── Backward Compatibility ──
pub mod telemetry {
    pub use crate::hardware::system_performance::*;
}

/// 🧬 cluaiz Bridge: Alias for get_sovereign_profile() for naming-migration compat.
pub fn get_cluaiz_profile() -> schema::profiles::cluaizProfile {
    get_sovereign_profile()
}

impl schema::profiles::SovereignProfile {
    /// 🧬 cluaiz Bridge: Alias for to_silicon_truth() for naming-migration compat.
    pub fn to_hardware_truth(&self) -> schema::profiles::SiliconTruth {
        self.to_silicon_truth()
    }
}
