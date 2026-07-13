//! 🚀 System Booster Orchestrator
//! Manages neural optimizations and synchronizes booster state via the HardwareGovernor.

use crate::manager::{ConflictResolver, AutoTuner};
use crate::{BoosterControl, HardwareGovernor};


pub struct SystemBooster;

impl SystemBooster {
    /// 📡 Ignite: Initializes the booster state and synchronizes with storage.
    pub fn ignite() -> anyhow::Result<BoosterControl> {
        let mut control = HardwareGovernor::load_booster_settings().unwrap_or_default();
        
        // ⚖️ Intelligent Orchestration (Boot-time)
        if let Ok(silicon) = HardwareGovernor::load_system_control().map(|s| s.silicon_truth) {
             // 🧪 1. Tune "Auto" states based on hardware
             AutoTuner::tune(&mut control, &silicon);

             // ⚖️ 2. Resolve initial conflicts
             ConflictResolver::resolve(&mut control, &silicon, &cluaiz_shared::backend::signature::KernelSignature::default());
        }

        // 🚀 OS Tuning: Elevate priority based on mode_run
        let priority_level = if control.mode_run == cluaiz_shared::hardware::schema::booster::BoosterMode::MaxBoost { "realtime" } else { "high" };
        let _ = crate::os_tuning::elevate_process_priority(priority_level);

        HardwareGovernor::save_booster_settings(&control)?;
        Ok(control)
    }

    /// ⚖️ Dynamic Resolve: Called after model loading to align with specific architecture.
    pub fn align_with_model(control: &mut BoosterControl, signature: &cluaiz_shared::backend::signature::KernelSignature) -> anyhow::Result<()> {
        let silicon = HardwareGovernor::load_system_control()?.silicon_truth;
        
        // ⚖️ Re-resolve based on specific model architecture
        ConflictResolver::resolve(control, &silicon, signature);
        
        Ok(())
    }



    /// 💾 Save: Persists the booster state (Proxied to Governor).
    pub fn save(control: &BoosterControl) -> anyhow::Result<()> {
        HardwareGovernor::save_booster_settings(control)
    }

    /// 📂 Load: Retrieves the existing booster state (Proxied to Governor).
    pub fn load() -> Option<BoosterControl> {
        HardwareGovernor::load_booster_settings().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_booster_ignition() {
        let result = SystemBooster::ignite();
        assert!(result.is_ok());
    }
}
