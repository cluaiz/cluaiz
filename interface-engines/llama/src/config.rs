//! 🚀 Sovereign Booster: Dynamic Configuration System
//! This module translates Registry-level capabilities into low-level engine parameters.

use crate::ffi::llama_cpp::{
    llama_context_default_params, llama_model_default_params, LlamaContextParams, LlamaModelParams,
};
use cluaiz_shared::hardware::schema::booster::{
    BoosterControl, BoosterMode, FeatureState, SmartState,
};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoosterConfig {
    #[serde(skip_serializing)]
    pub n_gpu_layers: i32,
    pub flash_attn: bool,
    #[serde(skip_serializing)]
    pub use_mmap: bool,
    #[serde(skip_serializing)]
    pub n_ctx: u32,
    #[serde(skip_serializing)]
    pub n_threads: i32,
    pub turbo_quant: String,
    pub dflash: String, // 🏛️ Delta Flash (FlashKDA Support)
    pub speculative_decoding: String,
    pub auto_round: String,
    pub mode_run: String,
    pub force_vram_reclaim: String,
    pub kv_cache_quantization: String,
    pub context_shifting: String,
    pub think_mode: String,
    pub force_memory_lock: String,
}

impl Default for BoosterConfig {
    fn default() -> Self {
        Self {
            n_gpu_layers: -1,
            flash_attn: true,
            use_mmap: true,
            n_ctx: 0,
            n_threads: -1,
            turbo_quant: "Auto".to_string(),
            dflash: "Auto".to_string(),
            speculative_decoding: "Off".to_string(),
            auto_round: "Auto".to_string(),
            mode_run: "balance".to_string(),
            force_vram_reclaim: "Off".to_string(),
            kv_cache_quantization: "Auto".to_string(),
            context_shifting: "Auto".to_string(),
            think_mode: "Auto".to_string(),
            force_memory_lock: "Auto".to_string(),
        }
    }
}

impl BoosterConfig {
    /// 🚀 Load the booster configuration from the sovereign system control.
    pub fn load_from_system() -> Self {
        // Default to Industrial Auto standards
        let mut config = Self {
            flash_attn: true,
            use_mmap: true,
            n_gpu_layers: -1, // Full Offload
            n_ctx: 0,         // Auto-detect from model
            n_threads: -1,    // Auto-detect CPU cores
            turbo_quant: "Auto".to_string(),
            dflash: "Auto".to_string(),
            speculative_decoding: "Off".to_string(),
            auto_round: "Auto".to_string(),
            mode_run: "balance".to_string(),
            force_vram_reclaim: "Off".to_string(),
            kv_cache_quantization: "Auto".to_string(),
            context_shifting: "Auto".to_string(),
            think_mode: "Auto".to_string(),
            force_memory_lock: "Auto".to_string(),
        };

        if let Ok(control) = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings() {
            config.flash_attn = control.flash_attention.is_active();
            config.n_gpu_layers = control.n_gpu_layers;
            config.dflash = match control.dflash {
                cluaiz_shared::hardware::schema::booster::SmartState::Static(s) => s,
                _ => "Auto".to_string(),
            };
            config.speculative_decoding = match control.speculative_decoding {
                cluaiz_shared::hardware::schema::booster::FeatureState::On => "On".to_string(),
                cluaiz_shared::hardware::schema::booster::FeatureState::Off => "Off".to_string(),
                _ => "Auto".to_string(),
            };
            config.auto_round = match control.auto_round {
                cluaiz_shared::hardware::schema::booster::FeatureState::On => "On".to_string(),
                cluaiz_shared::hardware::schema::booster::FeatureState::Off => "Off".to_string(),
                _ => "Auto".to_string(),
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
            config.think_mode = match control.think_mode {
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
        }
        config.use_mmap = true;
        config
    }
    /// 🛠️ Transform high-level config into raw model parameters.
    pub fn to_model_params(&self) -> LlamaModelParams {
        let mut params = unsafe { llama_model_default_params() };
        params.n_gpu_layers = self.n_gpu_layers;
        params.use_mmap = self.use_mmap;
        params.use_mlock = self.force_memory_lock == "On";
        params.no_host = self.n_gpu_layers != 0; // Avoid pinned host memory allocation if offloading layers to GPU
        params
    }

    /// 🛠️ Transform high-level config into raw context parameters.
    pub fn to_context_params(&self) -> LlamaContextParams {
        let mut params = unsafe { llama_context_default_params() };

        // 🛡️ Sovereign Context Handshake:
        // We use the requested context directly. The Governor's fitting loop
        // ensures this fits in VRAM before the engine is even initialized.
        params.n_ctx = self.n_ctx;

        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // 🛡️ CPU Thread Contention Fix:
        // available_parallelism() returns LOGICAL cores. Using all logical cores (HyperThreading)
        // causes severe cache thrashing and drops TPS to near 0 for LLMs (especially BitNet).
        // We MUST cap the threads to physical cores (roughly cores / 2).
        let physical_cores = if cores > 2 { cores / 2 } else { cores };
        
        let optimal_threads = match self.mode_run.to_lowercase().as_str() {
            "max_boost" | "ultra_max_boost" | "hyper_cluster" => physical_cores as i32,
            _ => {
                if physical_cores > 4 {
                    (physical_cores - 1).max(4) as i32 // Leave 1 core for OS in Balance mode
                } else {
                    physical_cores as i32
                }
            }
        };

        params.n_threads = if self.n_threads <= 0 {
            optimal_threads
        } else {
            self.n_threads
        };
        params.n_threads_batch = params.n_threads;

        // 🚀 KV-Cache Quantization Config:
        match self.kv_cache_quantization.to_lowercase().as_str() {
            "kv16" => {
                params.type_k = 1; // GGML_TYPE_F16
                params.type_v = 1;
            }
            "kv8" => {
                params.type_k = 8; // GGML_TYPE_Q8_0
                params.type_v = 8;
            }
            "kv4" => {
                params.type_k = 2; // GGML_TYPE_Q4_0
                params.type_v = 2;
            }
            _ => {
                // "Auto" (or "turbo_quant" fallback)
                if self.turbo_quant == "On" || self.turbo_quant == "Auto" {
                    params.type_k = 2; // GGML_TYPE_Q4_0
                    params.type_v = 2;
                } else {
                    params.type_k = 1; // GGML_TYPE_F16
                    params.type_v = 1;
                }
            }
        }

        let force_disable_fa_for_cpu = self.n_gpu_layers == 0;
        
        // 🛡️ Sovereign Safety Fallback:
        // Quantized KV cache requires flash attention enabled to load in VRAM and prevent init crashes.
        // HOWEVER, if the Sovereign Arbiter explicitly disabled Flash Attention (self.flash_attn == false)
        // or if we are forcing CPU mode (which disables FA), we MUST NOT force it back on.
        // We must instead gracefully fallback the KV Cache to F16.
        let mut is_quantized_kv = params.type_k == 8 || params.type_k == 2;
        if is_quantized_kv && (!self.flash_attn || force_disable_fa_for_cpu) {
            cluaiz_shared::dev_info!("⚠️ [Booster] KV Cache Quantization requires Flash Attention, but FA is disabled (or CPU mode forced). Falling back to F16 KV cache to prevent crash.");
            params.type_k = 1; // GGML_TYPE_F16
            params.type_v = 1;
            is_quantized_kv = false;
        }
        
        params.flash_attn_type = if (self.flash_attn || is_quantized_kv) && !force_disable_fa_for_cpu { 1 } else { 0 }; // 1 = LLAMA_FLASH_ATTN_TYPE_ENABLED
        params.offload_kqv = if self.n_gpu_layers == 0 { 0 } else { 1 }; // Force KV cache offload to VRAM only if GPU is enabled

        params
    }

    pub fn to_booster_control(&self) -> BoosterControl {
        BoosterControl {
            mode_run: match self.mode_run.to_lowercase().as_str() {
                "edge" => BoosterMode::Edge,
                "multitasking" => BoosterMode::Multitasking,
                "balance" => BoosterMode::Balance,
                "max_boost" => BoosterMode::MaxBoost,
                "ultra_max_boost" => BoosterMode::UltraMaxBoost,
                "hyper_cluster" => BoosterMode::HyperCluster,
                _ => BoosterMode::Balance,
            },
            turbo_quant: if self.turbo_quant == "On" {
                FeatureState::On
            } else if self.turbo_quant == "Off" {
                FeatureState::Off
            } else {
                FeatureState::Auto
            },
            flash_attention: if self.flash_attn {
                FeatureState::On
            } else {
                FeatureState::Off
            },
            speculative_decoding: if self.speculative_decoding == "On" {
                FeatureState::On
            } else if self.speculative_decoding == "Off" {
                FeatureState::Off
            } else {
                FeatureState::Auto
            },
            auto_round: if self.auto_round == "On" {
                FeatureState::On
            } else if self.auto_round == "Off" {
                FeatureState::Off
            } else {
                FeatureState::Auto
            },
            dflash: SmartState::Static(self.dflash.clone()),
            kv_cache_quantization: match self.kv_cache_quantization.to_lowercase().as_str() {
                "kv16" => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv16,
                "kv8" => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv8,
                "kv4" => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv4,
                _ => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Auto,
            },
            context_shifting: match self.context_shifting.to_lowercase().as_str() {
                "off" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Off,
                "minimal" => {
                    cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Minimal
                }
                "standard" | "on" => {
                    cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Standard
                }
                "aggressive" => {
                    cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Aggressive
                }
                "extreme" => {
                    cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Extreme
                }
                _ => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Auto,
            },
            force_vram_reclaim: if self.force_vram_reclaim == "On" {
                FeatureState::On
            } else {
                FeatureState::Off
            },
            n_gpu_layers: self.n_gpu_layers,
            think_mode: if self.think_mode == "On" {
                FeatureState::On
            } else if self.think_mode == "Off" {
                FeatureState::Off
            } else {
                FeatureState::Auto
            },
            response_length: "auto".to_string(),
            enforce_json: false,
            force_memory_lock: if self.force_memory_lock == "On" {
                FeatureState::On
            } else {
                FeatureState::Off
            },
        }
    }
}
