//! ⚖️ Conflict Resolver: Sovereign Decision Matrix
//! Handles synergies and overlaps between incompatible booster features.

use cluaiz_shared::hardware::schema::optimization::{OptimizationControl, FeatureState};
use cluaiz_shared::hardware::schema::profiles::SiliconTruth;
use cluaiz_shared::backend::signature::KernelSignature;

pub struct ConflictResolver;

impl ConflictResolver {
    /// 🧠 Resolves hardware and feature conflicts to prevent VRAM crashes or logic errors.
    pub fn resolve_and_apply(
        control: &mut OptimizationControl, 
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
            if vram_available < 12.0 && control.kv_cache_quantization == cluaiz_shared::hardware::schema::optimization::KvCacheQuantization::Auto {
                control.kv_cache_quantization = cluaiz_shared::hardware::schema::optimization::KvCacheQuantization::Kv4;
                println!("⚖️ [Manager] Low VRAM ({:.1}GB) detected. Calibrating KV Cache to Kv4 to support Speculative path.", vram_available);
            }
        }

        // 3. Synergy Check: Flash Attention + DFlash
        if control.speculative_decoding == FeatureState::On && control.flash_attention == FeatureState::Auto {
            control.flash_attention = FeatureState::On;
            println!("⚖️ [Manager] Synergistic optimization: Flash Attention forced ON for DFlash path.");
        }
    }
}
