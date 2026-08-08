//! 🚀 System Optimization Orchestrator
//! Manages neural optimizations and synchronizes optimization state via the HardwareGovernor.

use crate::manager::{ConflictResolver, AutoTuner};
use crate::{OptimizationControl, HardwareGovernor};

pub struct SystemOptimization;

impl SystemOptimization {
    /// 📡 Ignite: Initializes the optimization state and synchronizes with storage.
    pub fn ignite() -> anyhow::Result<OptimizationControl> {
        let mut control = HardwareGovernor::load_booster_settings().unwrap_or_default();
        
        // ⚖️ Intelligent Orchestration (Boot-time)
        if let Ok(silicon) = HardwareGovernor::load_system_control().map(|s| s.silicon_truth) {
             // 🧪 1. Tune "Auto" states based on hardware
             AutoTuner::tune(&mut control, &silicon);

             // ⚖️ 2. Resolve initial conflicts
             ConflictResolver::resolve_and_apply(&mut control, &silicon, &cluaiz_shared::backend::signature::KernelSignature::default());
        }

        // 🚀 OS Tuning: Elevate process priority for high throughput
        let priority_level = "high";
        let _ = crate::os_tuning::elevate_process_priority(priority_level);

        HardwareGovernor::save_booster_settings(&control)?;
        Ok(control)
    }

    /// ⚖️ Dynamic Resolve: Called after model loading to align with specific architecture.
    pub fn align_with_model(control: &mut OptimizationControl, signature: &cluaiz_shared::backend::signature::KernelSignature) -> anyhow::Result<()> {
        let silicon = HardwareGovernor::load_system_control()?.silicon_truth;
        
        // ⚖️ Re-resolve based on specific model architecture
        ConflictResolver::resolve_and_apply(control, &silicon, signature);
        
        Ok(())
    }

    /// 💾 Save: Persists the optimization state (Proxied to Governor).
    pub fn save(control: &OptimizationControl) -> anyhow::Result<()> {
        HardwareGovernor::save_booster_settings(control)
    }

    /// 📂 Load: Retrieves the existing optimization state (Proxied to Governor).
    pub fn load() -> Option<OptimizationControl> {
        HardwareGovernor::load_booster_settings().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimization_ignition() {
        let result = SystemOptimization::ignite();
        assert!(result.is_ok());
    }
}
