//! ⚖️ Conflict Resolver: Sovereign Decision Matrix
//! Handles synergies and overlaps between incompatible booster features.

use cluaiz_shared::hardware::schema::booster::{BoosterControl, FeatureState};
use cluaiz_shared::hardware::schema::profiles::SiliconTruth;
use cluaiz_shared::backend::signature::KernelSignature;

pub struct ConflictResolver;

impl ConflictResolver {
    /// 🧠 Resolves hardware and feature conflicts to prevent VRAM crashes or logic errors.
    pub fn resolve(
        control: &mut BoosterControl, 
        silicon: &SiliconTruth, 
        signature: &KernelSignature
    ) {
        println!("⚖️ [Manager] Initiating Conflict Resolution Protocol...");

        // 1. BitNet/SSM Optimization: Bypasses speculative paths for inherently efficient models.
        if signature.is_bitnet || signature.is_ssm {
            if control.speculative_decoding == FeatureState::On {
                control.speculative_decoding = FeatureState::Off;
                println!("⚖️ [Manager] BitNet/SSM detected. Speculative Decoding bypassed for native efficiency.");
            }
        }

        // 2. VRAM Resource Budgeting: Ensuring DFlash has breathing room.
        let vram_available = silicon.accelerators.gpus.iter().map(|g| g.vram_available_gb).sum::<f64>();
        
        if control.speculative_decoding == FeatureState::On {
            if vram_available < 12.0 {
                // Dependency: Speculative path on low VRAM requires TurboQuant to compress the context.
                if control.turbo_quant != FeatureState::On {
                    control.turbo_quant = FeatureState::On;
                    println!("⚖️ [Manager] Low VRAM ({:.1}GB) detected. Forcing TurboQuant=ON to support Speculative path.", vram_available);
                }
            }
        }

        // 3. Synergy Check: Flash Attention + DFlash
        if control.speculative_decoding == FeatureState::On && control.flash_attention == FeatureState::Auto {
            control.flash_attention = FeatureState::On;
            println!("⚖️ [Manager] Synergistic optimization: Flash Attention forced ON for DFlash path.");
        }
    }
}
