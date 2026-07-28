//! 🚀 Sovereign ONNX LLM Optimization Configuration System
//! Reads .cluaiz/engine/config/llm_optimization.json and maps Flash-Attn, KV-Cache, Memory Lock, and Concurrency settings directly to ORT Engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnnxLlmOptimizationConfig {
    pub mode_run: String,
    pub turbo_quant: String,
    pub flash_attention: String,
    pub speculative_decoding: String,
    pub auto_round: String,
    pub dflash: String,
    pub kv_cache_quantization: String,
    pub context_shifting: String,
    pub force_vram_reclaim: String,
    pub enforce_json: bool,
    pub force_memory_lock: String,
}

impl Default for OnnxLlmOptimizationConfig {
    fn default() -> Self {
        Self {
            mode_run: "hyper_cluster".to_string(),
            turbo_quant: "On".to_string(),
            flash_attention: "On".to_string(),
            speculative_decoding: "Off".to_string(),
            auto_round: "Auto".to_string(),
            dflash: "Auto".to_string(),
            kv_cache_quantization: "Auto".to_string(),
            context_shifting: "Auto".to_string(),
            force_vram_reclaim: "Off".to_string(),
            enforce_json: false,
            force_memory_lock: "Off".to_string(),
        }
    }
}

impl OnnxLlmOptimizationConfig {
    /// 🚀 Loads optimization configuration from system control (.cluaiz/engine/config/llm_optimization.json)
    pub fn load_from_system() -> Self {
        let mut config = Self::default();

        if let Ok(control) = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings() {
            config.flash_attention = if control.flash_attention.is_active() { "On".to_string() } else { "Off".to_string() };
            config.kv_cache_quantization = match control.kv_cache_quantization {
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Auto => "Auto".to_string(),
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv8 => "Kv8".to_string(),
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv4 => "Kv4".to_string(),
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv16 => "Kv16".to_string(),
            };
            config.context_shifting = match control.context_shifting {
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Off => "Off".to_string(),
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Minimal => "Minimal".to_string(),
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Standard => "Standard".to_string(),
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Aggressive => "Aggressive".to_string(),
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Extreme => "Extreme".to_string(),
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Auto => "Auto".to_string(),
            };
            config.mode_run = match control.mode_run {
                cluaiz_shared::hardware::schema::booster::BoosterMode::Edge => "edge".to_string(),
                cluaiz_shared::hardware::schema::booster::BoosterMode::Multitasking => "multitasking".to_string(),
                cluaiz_shared::hardware::schema::booster::BoosterMode::Balance => "balance".to_string(),
                cluaiz_shared::hardware::schema::booster::BoosterMode::MaxBoost => "max_boost".to_string(),
                cluaiz_shared::hardware::schema::booster::BoosterMode::UltraMaxBoost => "ultra_max_boost".to_string(),
                cluaiz_shared::hardware::schema::booster::BoosterMode::HyperCluster => "hyper_cluster".to_string(),
            };
            config.force_vram_reclaim = match control.force_vram_reclaim {
                cluaiz_shared::hardware::schema::booster::FeatureState::On => "On".to_string(),
                cluaiz_shared::hardware::schema::booster::FeatureState::Off => "Off".to_string(),
                _ => "Auto".to_string(),
            };
            config.force_memory_lock = match control.force_memory_lock {
                cluaiz_shared::hardware::schema::booster::FeatureState::On => "On".to_string(),
                cluaiz_shared::hardware::schema::booster::FeatureState::Off => "Off".to_string(),
                _ => "Auto".to_string(),
            };
            config.turbo_quant = match control.turbo_quant {
                cluaiz_shared::hardware::schema::booster::FeatureState::On => "On".to_string(),
                cluaiz_shared::hardware::schema::booster::FeatureState::Off => "Off".to_string(),
                _ => "Auto".to_string(),
            };

            tracing::info!(
                "⚙️ [ONNX LlmOptimization] Synchronized booster settings (FlashAttn: {}, KvQuant: {}, CtxShift: {}, Mode: {})",
                config.flash_attention, config.kv_cache_quantization, config.context_shifting, config.mode_run
            );
        } else {
            tracing::warn!("⚠️ [ONNX LlmOptimization] Could not load booster settings, falling back to defaults.");
        }

        config
    }

    pub fn is_flash_attention_enabled(&self) -> bool {
        self.flash_attention.eq_ignore_ascii_case("on") || self.flash_attention.eq_ignore_ascii_case("auto")
    }

    pub fn is_kv_cache_enabled(&self) -> bool {
        !self.kv_cache_quantization.eq_ignore_ascii_case("off")
    }

    pub fn is_context_shifting_enabled(&self) -> bool {
        !self.context_shifting.eq_ignore_ascii_case("off")
    }

    pub fn is_memory_lock_enabled(&self) -> bool {
        self.force_memory_lock.eq_ignore_ascii_case("on")
    }
}
