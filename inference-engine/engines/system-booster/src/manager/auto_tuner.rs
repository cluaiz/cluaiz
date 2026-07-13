//! 🧪 Auto Tuner: Hardware-Aware Feature Scaling
//! Calibrates "Auto" states based on real-time Silicon Truth.

use cluaiz_shared::hardware::schema::booster::{BoosterControl, FeatureState};
use cluaiz_shared::hardware::schema::profiles::SiliconTruth;

pub struct AutoTuner;

impl AutoTuner {
    /// 🧪 Calibrates all 'Auto' states into definitive On/Off positions based on hardware profile.
    pub fn tune(control: &mut BoosterControl, silicon: &SiliconTruth) {
        // Example: Auto Flash Attention on RTX cards
        if control.flash_attention == FeatureState::Auto {
            let has_cuda = silicon.accelerators.gpus.iter().any(|g| g.vendor.to_lowercase().contains("nvidia"));
            control.flash_attention = if has_cuda { FeatureState::On } else { FeatureState::Off };
            println!("🧪 [Manager] Auto-Tuner: Flash Attention calibrated to {:?}", control.flash_attention);
        }

        // Example: Auto TurboQuant for low VRAM
        if control.turbo_quant == FeatureState::Auto {
            let vram = silicon.accelerators.gpus.iter().map(|g| g.vram_available_gb).sum::<f64>();
            control.turbo_quant = if vram < 8.0 { FeatureState::On } else { FeatureState::Off };
            println!("🧪 [Manager] Auto-Tuner: TurboQuant calibrated to {:?}", control.turbo_quant);
        }
    }
}
