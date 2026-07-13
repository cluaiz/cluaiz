use super::SiliconTruth;
use anyhow::Result;
use cluaiz_shared::hardware::{governor::HardwareGovernor, system_control::HardwareOrchestrator};

/// 🏛️ Performs a deep surgical scan of the host Hardware.
pub fn detect_hardware() -> SiliconTruth {
    HardwareOrchestrator::probe().silicon_truth
}

/// 🛡️ Checks if the 'system_control.json' fingerprint exists.
pub fn has_config() -> bool {
    HardwareGovernor::start().is_ready()
}

/// 🧠 Reads the current hardware configuration.
pub fn read_config() -> Result<SiliconTruth> {
    // The governor maintains the system_control.json state.
    // For engine-level access, we provide the live Hardware profile.
    Ok(HardwareOrchestrator::probe().silicon_truth)
}

/// 📁 Persists the Hardware fingerprint to disk via the Governor.
pub fn save_config(_profile: &SiliconTruth) -> Result<()> {
    HardwareGovernor::auto_calibrate().map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

/// ⚙️ Updates a specific field in the cluaiz configuration.
pub fn update_field(field: &str, value: &str) -> Result<()> {
    // Convert string value to JSON for the Governor's update protocol.
    let val_json = serde_json::Value::String(value.to_string());
    HardwareGovernor::update_field(field, val_json).map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}
