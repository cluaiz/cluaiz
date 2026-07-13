//! 📦 Tier 7: Schema - Booster Control
//! Centralized Tri-State configuration for all system optimizations.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(check_bytes)]
#[serde(untagged)]
pub enum SmartState<T> {
    Static(String),
    Custom(T),
}

impl<T> SmartState<T> {
    pub fn is_active(&self) -> bool {
        match self {
            SmartState::Static(s) => s == "On" || s == "Auto",
            SmartState::Custom(_) => true, // Custom config implies active
        }
    }
}

impl<T> Default for SmartState<T> {
    fn default() -> Self {
        SmartState::Static("Auto".into())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
    PartialEq,
    Default,
)]
#[archive(check_bytes)]
pub enum FeatureState {
    On,
    Off,
    #[default]
    Auto,
}

impl FeatureState {
    pub fn is_active(&self) -> bool {
        matches!(self, FeatureState::On | FeatureState::Auto)
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Default, PartialEq, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(check_bytes)]
pub struct DFlashConfig {
    pub state: String,       // Always "On" in this variant
    pub budget: u32,         // Safe user-tunable knob
    pub asymmetric_kv: bool, // TQ3_0 for K, F16 for V
    pub draft_model_path: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
    PartialEq,
    Default,
)]
#[archive(check_bytes)]
pub enum KvCacheQuantization {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto,
    #[serde(rename = "Kv16", alias = "kv16")]
    Kv16,
    #[serde(rename = "Kv8", alias = "kv8")]
    Kv8,
    #[serde(rename = "Kv4", alias = "kv4")]
    Kv4,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
    PartialEq,
    Default,
)]
#[archive(check_bytes)]
pub enum ContextShiftingMode {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto, // Defaults to Balanced (10%)
    #[serde(rename = "Off", alias = "off")]
    Off,
    #[serde(rename = "Minimal", alias = "minimal")]
    Minimal, // 5% shift
    #[serde(rename = "Standard", alias = "standard", alias = "on", alias = "On")]
    Standard, // 10% shift
    #[serde(rename = "Aggressive", alias = "aggressive")]
    Aggressive, // 25% shift
    #[serde(rename = "Extreme", alias = "extreme")]
    Extreme, // 50% shift
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
    PartialEq,
    Default,
)]
#[archive(check_bytes)]
pub enum BoosterMode {
    #[serde(rename = "edge")]
    Edge, // 📱 Mobile/NPU/Pi (Extreme pruning)
    #[default]
    #[serde(rename = "multitasking")]
    Multitasking, // 💻 Standard Laptop (Respects OS/Apps)
    #[serde(rename = "balance")]
    Balance, // ⚖️ Standard Performance
    #[serde(rename = "max_boost")]
    MaxBoost, // 🚀 AI Priority (Workstation)
    #[serde(rename = "ultra_max_boost")]
    UltraMaxBoost, // 🔥 Reclaims everything (Formerly Landlord)
    #[serde(rename = "hyper_cluster")]
    HyperCluster, // 🌌 Server/H100 Cluster (Zero-margin orchestration)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct BoosterControl {
    pub mode_run: BoosterMode,
    pub turbo_quant: FeatureState,
    pub flash_attention: FeatureState,
    pub speculative_decoding: FeatureState,
    pub auto_round: FeatureState,
    pub dflash: SmartState<DFlashConfig>,
    pub kv_cache_quantization: KvCacheQuantization,
    pub context_shifting: ContextShiftingMode,
    pub force_vram_reclaim: FeatureState, // 🏠 'Landlord' Mode Flag
    #[serde(default = "default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    #[serde(default)]
    pub think_mode: FeatureState,
    #[serde(default)]
    pub response_length: String, // "auto", "short", "long"
    #[serde(default)]
    pub enforce_json: bool, // Strict Grammar Masking trigger
    #[serde(default)]
    pub force_memory_lock: FeatureState, // OS VirtualLock / mlock
}

fn default_n_gpu_layers() -> i32 {
    -1
}

impl BoosterControl {
    /// 🧠 The Conflict Resolution Manager
    pub fn resolve_conflicts(
        &mut self,
        silicon: &crate::hardware::schema::profiles::SiliconTruth,
        signature: &crate::backend::signature::KernelSignature,
    ) {
        // 1. BitNet/SSM Constraints
        if signature.is_bitnet || signature.is_ssm {
            // BitNet doesn't need DFlash/Speculative usually
            self.speculative_decoding = FeatureState::Off;
        }

        // 2. VRAM Dependency
        let vram_available = silicon
            .accelerators
            .gpus
            .iter()
            .map(|g| g.vram_available_gb)
            .sum::<f64>();

        if self.speculative_decoding == FeatureState::On {
            if vram_available < 12.0 {
                // Force TurboQuant ON to fit draft model
                self.turbo_quant = FeatureState::On;
                println!("⚖️ [ConflictManager] Low VRAM ({:.1}GB) detected. Forcing TurboQuant = ON for Speculative Decoding.", vram_available);
            }
            // Flash Attention synergy
            if self.flash_attention == FeatureState::Auto {
                self.flash_attention = FeatureState::On;
            }
        }

        // 3. Universal Memory Lock Trigger

        if vram_available <= 6.0 && self.force_memory_lock == FeatureState::Auto {
            self.force_memory_lock = FeatureState::On;
            println!("🔒 [Arbiter] Low VRAM ({:.1}GB). Auto-enabling OS Memory Lock (mlock) to prevent page-file swap.", vram_available);
        }
    }
}

impl Default for BoosterControl {
    fn default() -> Self {
        Self {
            mode_run: BoosterMode::Balance,
            turbo_quant: FeatureState::Auto,
            flash_attention: FeatureState::Auto,
            speculative_decoding: FeatureState::Off,
            auto_round: FeatureState::Auto,
            dflash: SmartState::Static("Auto".into()),
            kv_cache_quantization: KvCacheQuantization::Auto,
            context_shifting: ContextShiftingMode::Auto,
            force_vram_reclaim: FeatureState::Off,
            n_gpu_layers: -1,
            think_mode: FeatureState::Auto,
            response_length: "auto".to_string(),
            enforce_json: false,
            force_memory_lock: FeatureState::Off,
        }
    }
}

/// 🚀 FFI-Compatible C-Struct for injecting Booster configurations into C++ Kernels (llama.cpp)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cluaizBoosterContext {
    pub turbo_quant: bool,
    pub flash_attention: bool,
    // Provide integer-based flags for C++ FFI compatibility
    pub speculative_decoding_mode: u8,  // 0 = Off, 1 = On, 2 = Auto
    pub kv_cache_quantization_mode: u8, // 0 = Auto/Kv16, 1 = Kv8, 2 = Kv4
    pub context_shifting_mode: u8,      // 0 = Off, 1 = Small, 2 = Balanced, 3 = Boost, 4 = Ultra
    pub n_gpu_layers: i32,
    pub force_memory_lock: bool,
    pub max_context_length: u32,
}

impl From<&BoosterControl> for cluaizBoosterContext {
    fn from(config: &BoosterControl) -> Self {
        let kv_mode = match config.kv_cache_quantization {
            KvCacheQuantization::Auto | KvCacheQuantization::Kv16 => 0,
            KvCacheQuantization::Kv8 => 1,
            KvCacheQuantization::Kv4 => 2,
        };

        let shift_mode = match config.context_shifting {
            ContextShiftingMode::Off => 0,
            ContextShiftingMode::Minimal => 1,
            ContextShiftingMode::Auto | ContextShiftingMode::Standard => 2,
            ContextShiftingMode::Aggressive => 3,
            ContextShiftingMode::Extreme => 4,
        };

        let spec_mode = match config.speculative_decoding {
            FeatureState::Off => 0,
            FeatureState::On => 1,
            FeatureState::Auto => 2,
        };

        Self {
            turbo_quant: config.turbo_quant == FeatureState::On,
            flash_attention: config.flash_attention == FeatureState::On,
            speculative_decoding_mode: spec_mode,
            kv_cache_quantization_mode: kv_mode,
            context_shifting_mode: shift_mode,
            n_gpu_layers: config.n_gpu_layers,
            force_memory_lock: config.force_memory_lock == FeatureState::On,
            max_context_length: 0,
        }
    }
}
