//! 🧠 Universal Context Negotiator (Single Source of Truth)
//! Dedicated modular engine for dynamic context window resolution across ALL tiers:
//! - Tier 1: GPU Only
//! - Tier 2: Hybrid VRAM + RAM
//! - Tier 3: CPU RAM Only
//! - Tier 4: SSD Streaming (MoE)
//!
//! Enforces:
//! 1. Strictly ONE static floor: 2048 tokens (`min_2k_tokens`).
//! 2. Reads model native context limit from `model_registry.json`.
//! 3. Protects GGML workspace reserve and Windows OS safety buffers.
//! 4. Handles user settings (Auto=0, Full=-1, Custom=N) uniformly.

use std::path::Path;

/// The final context resolution decision produced by the Sovereign Arbiter.
#[derive(Debug, Clone)]
pub struct ContextResolution {
    /// Negotiated target context window in tokens (Single Source of Truth, minimum 2048)
    pub target_ctx_tokens: usize,
    /// Human-readable log string describing the resolution mode
    pub ctx_mode_str: String,
    /// RAM required for the KV cache at this context size in GB
    pub required_ctx_gb: f64,
    /// Native context limit discovered from model registry or defaults
    pub native_max_ctx: usize,
    /// Exact bytes per token dynamically calculated for the model's KV cache
    pub kv_bytes_per_token: f64,
}

/// Discovers the model's native context limit from `model_registry.json`.
pub fn get_model_native_context(model_path: &Path) -> usize {
    let default_native_ctx = 32768usize;
    let reg_path = crate::environment::EnvironmentManager::current()
        .config_dir()
        .join("model_registry.json");

    if let Ok(content) = std::fs::read_to_string(&reg_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(installed) = val.get("installed_models").and_then(|m| m.as_object()) {
                let target_dir_str = model_path
                    .to_string_lossy()
                    .to_lowercase()
                    .replace('\\', "/");
                for (_id, entry) in installed {
                    let local_dir = entry
                        .get("local_dir")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_lowercase()
                        .replace('\\', "/");

                    if !local_dir.is_empty()
                        && (local_dir == target_dir_str
                            || target_dir_str.contains(&local_dir)
                            || local_dir.contains(&target_dir_str))
                    {
                        if let Some(meta) = entry.get("metadata") {
                            if let Some(ctx_val) = meta
                                .get("context_window")
                                .or_else(|| meta.get("context_length"))
                            {
                                if let Some(ctx_u) = ctx_val.as_u64() {
                                    if ctx_u > 0 {
                                        return ctx_u as usize;
                                    }
                                } else if let Some(ctx_s) = ctx_val.as_str() {
                                    let cleaned = ctx_s.trim().to_uppercase();
                                    if cleaned.ends_with('K') {
                                        if let Ok(k) =
                                            cleaned.trim_end_matches('K').parse::<usize>()
                                        {
                                            return k * 1024;
                                        }
                                    } else if let Ok(num) = cleaned.parse::<usize>() {
                                        return num;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    default_native_ctx
}

/// Dynamically calculates the safe target context window (in tokens) for ANY model and tier.
///
/// Single Source of Truth:
/// - Only 1 static floor: 2048 minimum tokens.
/// - Dynamic ceiling clamped by native context length and usable memory headroom.
pub fn resolve_context_window(
    model_path: &Path,
    user_n_ctx: i32,
    usable_ram_gb: f64,
    total_ram_gb: f64,
    model_size_gb: f64,
    reserved_non_ctx_gb: f64,
) -> ContextResolution {
    let min_2k_tokens = 2048usize;

    // Load optimization settings for KV cache quantization format
    let opt_control = crate::hardware::governor::HardwareGovernor::load_optimization_settings().unwrap_or_default();
    let element_bytes = match opt_control.kv_cache_quantization {
        crate::hardware::schema::optimization::KvCacheQuantization::Kv4 => 0.5f64,
        crate::hardware::schema::optimization::KvCacheQuantization::Kv8 => 1.0f64,
        _ => 2.0f64, // Kv16 or Auto default (FP16)
    };

    // 🔬 Pure Mathematical Truth: Probe model geometry directly from GGUF binary
    let (kv_bytes_per_token, native_max_ctx) = if model_path.exists() && model_path.is_file() {
        if let Ok(arch_info) = crate::metadata::GgufBinaryProber::probe(model_path) {
            let bytes_per_tok = arch_info.kv_bytes_per_token(element_bytes);
            let native_ctx = arch_info.context_length.max(min_2k_tokens);
            (bytes_per_tok, native_ctx)
        } else {
            let native_ctx = get_model_native_context(model_path).max(min_2k_tokens);
            (2.0 * 32.0 * 8.0 * 128.0 * element_bytes, native_ctx)
        }
    } else {
        let native_ctx = get_model_native_context(model_path).max(min_2k_tokens);
        (2.0 * 32.0 * 8.0 * 128.0 * element_bytes, native_ctx)
    };

    // Dynamic GGML Workspace Reserve (Scales with model size)
    let ggml_workspace_reserve = (0.25 + (model_size_gb * 0.04)).clamp(0.50, 3.00);

    // Usable RAM already includes the OS safety buffer deducted centrally by memory_governor.
    let total_reserved = ggml_workspace_reserve + reserved_non_ctx_gb;
    let ram_for_ctx = (usable_ram_gb - total_reserved).max(0.0);

    let max_possible_tokens =
        ((ram_for_ctx * 1024.0 * 1024.0 * 1024.0) / kv_bytes_per_token) as usize;

    let (target_ctx_tokens, ctx_mode_str) = match user_n_ctx {
        -1 | i32::MAX => {
            let safe = max_possible_tokens.clamp(min_2k_tokens, native_max_ctx);
            let label = if safe == native_max_ctx {
                "Full Window"
            } else {
                "Clamped"
            };
            (safe, format!("{} -> {} Tokens", label, safe))
        }
        n if n > 0 => {
            let req = n as usize;
            let safe = req
                .min(max_possible_tokens)
                .clamp(min_2k_tokens, native_max_ctx);
            let label = if safe == req { "Custom" } else { "Clamped" };
            (safe, format!("{} -> {} Tokens", label, safe))
        }
        _ => {
            // Auto Mode: 100% Dynamic Scaling.
            // Bounded strictly by available physical RAM headroom and model native context length.
            let safe = max_possible_tokens.clamp(min_2k_tokens, native_max_ctx);
            (safe, format!("Auto Dynamic ({} Tokens)", safe))
        }
    };

    let required_ctx_gb =
        (target_ctx_tokens as f64 * kv_bytes_per_token) / (1024.0 * 1024.0 * 1024.0);

    ContextResolution {
        target_ctx_tokens,
        ctx_mode_str,
        required_ctx_gb,
        native_max_ctx,
        kv_bytes_per_token,
    }
}
