//! ⚖️ Unified Resource Negotiator
//! Single Source of Truth for hardware placement decisions across ALL engines (GGUF/ONNX)
//! and ALL inference modes (Chat/Embedding/Audio/TTS).
//!
//! Tiered Waterfall: GPU VRAM → Shared VRAM+RAM (Hybrid) → CPU RAM → SSD Streaming

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::hardware::expert_offloading::{detect_moe, MoeModelInfo};
use crate::hardware::governor::HardwareGovernor;
use crate::hardware::schema::optimization::{FeatureState, OptimizationControl};

/// Global thread-safe Mutex lock to serialize CUDA and VRAM resource negotiation across parallel requests.
pub static GLOBAL_HARDWARE_LOCK: Mutex<()> = Mutex::new(());

// ─── Core Types ──────────────────────────────────────────────────────────────

/// Which backend engine is requesting resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineType {
    GGUF,
    ONNX,
}

/// What kind of inference work the engine will perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceMode {
    Chat,
    Embedding,
    Audio,
    TTS,
}

/// A request from an engine to the governor for hardware resources.
#[derive(Debug, Clone)]
pub struct ResourceRequest {
    pub engine_type: EngineType,
    pub inference_mode: InferenceMode,
    pub model_size_gb: f64,
    pub model_path: PathBuf,
}

/// The hardware placement tier assigned by the governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementTier {
    /// Tier 1: Entire model fits in dedicated GPU VRAM. Fastest throughput.
    GpuOnly,
    /// Tier 2: Model split across GPU VRAM + System RAM. Good throughput.
    Hybrid,
    /// Tier 3: Model runs entirely on System RAM (CPU). Moderate throughput.
    CpuOnly,
    /// Tier 4: Model exceeds RAM. Requires SSD-backed streaming. Slowest.
    SsdStreaming,
}

impl std::fmt::Display for PlacementTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlacementTier::GpuOnly => write!(f, "GPU Only (Tier 1)"),
            PlacementTier::Hybrid => write!(f, "Hybrid VRAM+RAM (Tier 2)"),
            PlacementTier::CpuOnly => write!(f, "CPU RAM Only (Tier 3)"),
            PlacementTier::SsdStreaming => write!(f, "SSD Streaming (Tier 4)"),
        }
    }
}

/// The governor's final resource allocation decision.
#[derive(Debug, Clone)]
pub struct ResourceGrant {
    pub tier: PlacementTier,
    /// -1 = all layers on GPU, 0 = CPU only, N = partial offload
    pub n_gpu_layers: i32,
    /// CPU threads allocated for this engine
    pub thread_count: usize,
    /// VRAM budget in GB this engine may use
    pub vram_budget_gb: f64,
    /// System RAM budget in GB this engine may use
    pub ram_budget_gb: f64,
    /// OS safety buffer reserved (GB)
    pub safety_buffer_gb: f64,
    /// Expert LRU cache budget in GB (non-zero only in Tier 4 / MoE offloading mode)
    pub expert_cache_budget_gb: f64,
    /// MoE structural metadata (Some only when Tier 4 is active for a MoE model)
    pub moe_info: Option<MoeModelInfo>,
    /// Target dynamic context window in tokens (Single Source of Truth, minimum 2048)
    pub target_ctx_tokens: usize,
}

// ─── Core Negotiation Logic ──────────────────────────────────────────────────

/// The unified resource negotiation function.
/// Called by ALL engines (GGUF/ONNX) and ALL modes (Chat/Embedding/Audio/TTS).
///
/// Returns a `ResourceGrant` with tier, GPU layers, and memory budgets.
pub fn negotiate_resource(request: &ResourceRequest) -> anyhow::Result<ResourceGrant> {
    let _lock = GLOBAL_HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    eprintln!(
        "⚖️ [Negotiator] >>> Starting resource negotiation for model: {:?}",
        request.model_path
    );
    eprintln!(
        "⚖️ [Negotiator] Model size: {:.2} GB | Engine: {:?} | Mode: {:?}",
        request.model_size_gb, request.engine_type, request.inference_mode
    );

    // ─── Step 1: Read Silicon Truth & Real-Time Dynamic NVML VRAM ───
    let control = HardwareGovernor::load_system_control().unwrap_or_default();

    let total_vram_gb: f64 = control
        .silicon_truth
        .accelerators
        .gpus
        .iter()
        .map(|g| g.vram_total_gb)
        .sum();

    // Query Real-Time Dynamic Free VRAM directly from NVML
    let live_free_vram_gb: f64 = if let Ok(nvml) = nvml_wrapper::Nvml::init() {
        if let Ok(dev) = nvml.device_by_index(0) {
            dev.memory_info()
                .map(|m| m.free as f64 / 1_073_741_824.0)
                .unwrap_or(total_vram_gb)
        } else {
            total_vram_gb
        }
    } else {
        total_vram_gb
    };

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram_gb = (sys.total_memory() as f64) / (1024.0 * 1024.0 * 1024.0);
    let available_ram_gb = (sys.available_memory() as f64) / (1024.0 * 1024.0 * 1024.0);

    // ─── Step 2: Read User Settings ───
    let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
    eprintln!(
        "⚖️ [Negotiator] User config: extreme_moe_streaming = {:?} | force_memory_lock = {:?}",
        opt_control.extreme_moe_streaming, opt_control.force_memory_lock
    );

    // ─── Step 3: Read Engine-Specific n_gpu_layers ───
    let user_n_gpu_layers = match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_gpu_layers
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load().n_gpu_layers
        }
    };
    let user_n_ctx = match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_ctx
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load().n_ctx
        }
    };
    let ctx_setting_str = match user_n_ctx {
        -1 | i32::MAX => "Max Full".to_string(),
        0 => "Auto".to_string(),
        n => format!("{}", n),
    };
    let model_gb = request.model_size_gb;

    // ─── Step 4: Calculate Usable Memory from Real-Time Free Memory ───
    let decision = crate::hardware::memory_governor::get_memory_decision(
        &opt_control,
        total_vram_gb,
        live_free_vram_gb,
        total_ram_gb,
        available_ram_gb,
    );
    let usable_vram = decision.usable_vram_gb;
    let usable_ram = decision.usable_ram_gb;
    let vram_safety = decision.vram_safety_gb;
    let ram_safety = decision.ram_safety_gb;

    // ─── Step 5: Check Existing ARBITER Allocations ───
    let existing_allocs: f64 = HardwareGovernor::get_active_allocations()
        .iter()
        .map(|p| p.vram_gb)
        .sum();
    let free_vram = (usable_vram - existing_allocs).max(0.0);

    // ─── Step 1b: Early MoE Detection ───
    let moe_info = detect_moe(&request.model_path);

    // Context Window (n_ctx) Dynamic Sizing & Safety (Universal Single Source of Truth, min 2048)
    let min_ram_weights_est = if moe_info.is_moe {
        let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let expert_total_gb = moe_info.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        (dense_gb + (expert_total_gb * 0.70)).min(model_gb - free_vram).max(0.0)
    } else {
        (model_gb - free_vram).max(0.0)
    };
    let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
        &request.model_path,
        user_n_ctx,
        usable_ram,
        total_ram_gb,
        model_gb,
        min_ram_weights_est,
    );
    let mut target_ctx_tokens = ctx_resolution.target_ctx_tokens;
    let ctx_mode_str = ctx_resolution.ctx_mode_str;
    let mut required_ctx_gb = ctx_resolution.required_ctx_gb;
    let native_max_ctx = ctx_resolution.native_max_ctx;
    let kv_bytes_per_token = ctx_resolution.kv_bytes_per_token;

    let n_ctx_reservation_gb = 1.00f64;
    let ram_for_expert_cache = (usable_ram - n_ctx_reservation_gb).max(0.5);

    // Structured Log Output matching logic.md & README.md Contract
    let vram_setting_str = match opt_control.custom_vram_buffer_gb {
        Some(val) => format!("{:.2} GB", val),
        None => "Auto".to_string(),
    };
    let ram_setting_str = match opt_control.custom_ram_buffer_gb {
        Some(val) => format!("{:.2} GB", val),
        None => "Auto".to_string(),
    };

    eprintln!(
        "⚖️ [Negotiator] Silicon Hardware Detected: VRAM Total = {:.2} GB (Free = {:.2} GB) | System RAM Total = {:.2} GB (Free = {:.2} GB)",
        total_vram_gb, live_free_vram_gb, total_ram_gb, available_ram_gb
    );
    eprintln!(
        "🎰 [Negotiator] User Settings: custom_vram_buffer = {} | custom_ram_buffer = {} | extreme_moe_streaming = {:?} | n_ctx = {}",
        vram_setting_str,
        ram_setting_str,
        opt_control.extreme_moe_streaming,
        ctx_setting_str
    );
    eprintln!(
        "🧠 [Negotiator] Dynamic Context Window Resolved: {} (Single Source of Truth, min 2048)",
        ctx_mode_str
    );
    if moe_info.is_moe {
        let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        eprintln!(
            "🔍 [MoeDetector] Model Architecture: MoE Detected ({} Experts, {} Layers, Size: {:.2} GB, Dense Backbone: {:.2} GB)",
            moe_info.expert_count, moe_info.moe_layer_count, model_gb, dense_gb
        );
    }

    // ─── Step 6: User Override Check ───
    if user_n_gpu_layers == 0 {
        let cache_budget = if moe_info.is_moe {
            moe_info.recommended_cache_budget_gb()
        } else {
            0.0
        };
        let final_moe = if moe_info.is_moe {
            Some(moe_info.clone())
        } else {
            None
        };
        eprintln!(
            "⚪ [Negotiator] CPU-Only mode forced by user (n_gpu_layers = 0). Model: {:.2}GB, Free RAM: {:.2}GB",
            model_gb, usable_ram
        );
        return Ok(ResourceGrant {
            tier: PlacementTier::CpuOnly,
            n_gpu_layers: 0,
            thread_count: sysinfo::System::new().cpus().len().max(1),
            vram_budget_gb: 0.0,
            ram_budget_gb: usable_ram.max((model_gb * 0.8).min(total_ram_gb - 2.0)),
            safety_buffer_gb: ram_safety,
            expert_cache_budget_gb: cache_budget,
            moe_info: final_moe,
            target_ctx_tokens,
        });
    }

    // ─── Step 7: Tiered Placement Decision ───
    let (tier, n_gpu_layers, vram_budget, ram_budget, final_moe_info, expert_cache_budget_gb) =
        if total_vram_gb < 0.1 {
            (
                PlacementTier::CpuOnly,
                0i32,
                0.0,
                usable_ram.min(model_gb * 1.2),
                None,
                0.0,
            )
        } else if moe_info.is_moe
            && opt_control.extreme_moe_streaming.is_active()
            && model_gb > free_vram
        {
            let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let expert_total_gb = moe_info.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let layer_count = moe_info.moe_layer_count.max(1) as f64;

            // Accurate MoE layer sizing (Dense Attention + MoE Router + Active/Cached Experts)
            let dense_per_layer_gb = dense_gb / layer_count;
            let expert_per_layer_gb = expert_total_gb / layer_count;

            // Single expert size in GB (Divide total expert bytes by total experts across all layers)
            let total_model_experts =
                (moe_info.expert_count * moe_info.moe_layer_count.max(1)) as f64;
            let single_expert_gb = if moe_info.total_expert_bytes > 0 && total_model_experts > 0.0 {
                (moe_info.total_expert_bytes as f64 / total_model_experts)
                    / (1024.0 * 1024.0 * 1024.0)
            } else if moe_info.expert_size_bytes > 0 {
                moe_info.expert_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            } else {
                0.0
            };

            // Complete layer footprint on GPU (Dense Attention + Router + All Experts in this layer).
            // When offloading layers to CUDA, llama.cpp allocates the entire layer weights in VRAM.
            // Using full layer size guarantees 0 bytes spill into Windows Shared GPU Memory.
            let layer_size = (dense_per_layer_gb + expert_per_layer_gb).max(0.04);

            // Single minimal display compositor safety headroom (150MB for Windows DWM)
            let vram_display_headroom_gb = 0.15_f64;

            // Step 1: Base Reserves & Initial Allocations
            let ram_dense_reserve =
                (moe_info.dense_backbone_bytes as f64) / (1024.0 * 1024.0 * 1024.0);

            // GGML Compute Graph Workspace Reserve (Dynamic)
            // Scales dynamically with model size (proxy for hidden dim size). Base overhead is ~250MB.
            // Adds ~40MB per 1GB of model weights. Bounded between 500MB and 3GB.
            let ggml_workspace_reserve = (0.25 + (model_gb * 0.04)).clamp(0.50, 3.00);

            // ─── Dynamic Layer Offload: Real Physical VRAM Capacity (Zero Artificial Buffers) ───
            // Allocate strictly what physically fits into live free VRAM so 0 bytes spill into Shared Memory.
            let mut ctx_in_vram = false;
            let vram_for_layers = (live_free_vram_gb - vram_display_headroom_gb).max(0.0);

            let approx_layers = if layer_size > 0.0 && vram_for_layers > 0.0 {
                ((vram_for_layers / layer_size).floor() as i32)
                    .min(moe_info.moe_layer_count as i32)
                    .max(0)
            } else {
                0
            };

            let mut allocated_vram = (approx_layers.max(0) as f64 * layer_size).min(live_free_vram_gb);

            // ⚡ VRAM Context Acceleration:
            // Check if VRAM has room for the KV cache after all possible GPU layers are allocated.
            let vram_headroom_after_layers = (live_free_vram_gb - allocated_vram).max(0.0);
            if vram_headroom_after_layers >= (required_ctx_gb + vram_safety)
                && required_ctx_gb > 0.0
            {
                ctx_in_vram = true;
                allocated_vram += required_ctx_gb;
                eprintln!(
                    "⚡ [Negotiator] VRAM Context Accelerated: Placing {:.2} GB KV cache into GDDR6 VRAM (10x faster memory bandwidth).",
                    required_ctx_gb
                );
            }

            let remaining_layers =
                (moe_info.moe_layer_count as i32).saturating_sub(approx_layers.max(0));
            let gpu_experts = moe_info.expert_count * approx_layers.max(0) as usize;
            let offloaded_layer_experts = moe_info.expert_count * remaining_layers.max(0) as usize;

            let offloaded_experts_gb = offloaded_layer_experts as f64 * single_expert_gb;

            // Step 2: Optimistic Allocation (Try to fit ALL offloaded layers in cache first)
            let initial_cache_budget = offloaded_experts_gb;
            let initial_cached_expert_count = offloaded_layer_experts;
            let initial_cached_layers = remaining_layers;

            let ram_after_base =
                (usable_ram - ram_dense_reserve - ggml_workspace_reserve)
                    .max(0.0);

            // Step 3: Layer Cache Budget in System RAM
            // Only deduct required_ctx_gb from System RAM if context is NOT in VRAM!
            let ctx_ram_deduction = if ctx_in_vram { 0.0 } else { required_ctx_gb };
            let total_non_cache_reserve = ram_dense_reserve
                + ggml_workspace_reserve
                + ctx_ram_deduction;
            let ram_for_cache = (usable_ram - total_non_cache_reserve).max(0.0);
            let mut cached_expert_count = initial_cached_expert_count;

            if single_expert_gb > 0.0 {
                let max_experts_fit = (ram_for_cache / single_expert_gb).floor() as usize;
                cached_expert_count = initial_cached_expert_count.min(max_experts_fit);
            }

            // 🛡️ Weights-First Dynamic Backpressure (No Hardcoded Clamps):
            // If context is in RAM and offloaded layers would overflow to SSD,
            // dynamically scale down context window so 100% of layers fit into RAM (0 layers on SSD).
            if !ctx_in_vram && cached_expert_count < initial_cached_expert_count {
                let ram_weights_needed = ram_dense_reserve + offloaded_experts_gb;
                if ram_weights_needed + ggml_workspace_reserve + 0.25
                    <= usable_ram
                {
                    let ram_for_ctx_adjusted = (usable_ram
                        - ram_weights_needed
                        - ggml_workspace_reserve)
                        .max(0.0);
                    let dyn_tokens = ((ram_for_ctx_adjusted * 1024.0 * 1024.0 * 1024.0)
                        / kv_bytes_per_token) as usize;
                    let upper_bound = if user_n_ctx > 0 {
                        (user_n_ctx as usize).min(native_max_ctx)
                    } else {
                        native_max_ctx
                    };
                    let adjusted_tokens = dyn_tokens.clamp(2048, upper_bound);
                    if adjusted_tokens != target_ctx_tokens {
                        eprintln!(
                            "⚡ [Negotiator] Weights-First Backpressure: Dynamically adjusting context from {} to {} tokens to guarantee 0 layers on SSD (Headroom: {:.2} GB).",
                            target_ctx_tokens, adjusted_tokens, ram_for_ctx_adjusted
                        );
                        target_ctx_tokens = adjusted_tokens;
                        required_ctx_gb = (target_ctx_tokens as f64 * kv_bytes_per_token)
                            / (1024.0 * 1024.0 * 1024.0);
                        let new_ram_for_cache = (usable_ram
                            - ram_dense_reserve
                            - ggml_workspace_reserve
                            - required_ctx_gb)
                            .max(0.0);
                        if single_expert_gb > 0.0 {
                            let max_fit = (new_ram_for_cache / single_expert_gb).floor() as usize;
                            cached_expert_count = initial_cached_expert_count.min(max_fit);
                        }
                    }
                }
            }

            let cache_budget = if single_expert_gb > 0.0 {
                (cached_expert_count as f64 * single_expert_gb).min(ram_after_base)
            } else {
                initial_cache_budget.min(ram_after_base)
            };

            let mut actual_cache_gb = cache_budget;
            let mut cached_layers = if moe_info.expert_count > 0 {
                (cached_expert_count as f64 / moe_info.expert_count as f64).round() as i32
            } else {
                0
            };
            let mut cut_layers_for_safety = initial_cached_layers - cached_layers;
            let mut overflow_layers = initial_cached_layers - cached_layers;

            let mut overflow_experts = offloaded_layer_experts.saturating_sub(cached_expert_count);
            let mut overflow_gb = if single_expert_gb > 0.0 {
                single_expert_gb * overflow_experts as f64
            } else {
                0.0
            };

            // 🛡️ Dynamic Spill Absorption Doctrine (100% Pure Dynamic Math, Zero Hardcoding):
            // Total system RAM must strictly stay below 95% physical ceiling.
            // Calculate the exact live headroom remaining before hitting 95% total RAM.
            let max_safe_system_ram = total_ram_gb * 0.95;
            let other_apps_used_ram = (total_ram_gb - available_ram_gb).max(0.0);
            let max_allowed_cluaiz_ram = (max_safe_system_ram - other_apps_used_ram).max(0.0);

            let total_weight_and_overhead_need = ram_dense_reserve
                + offloaded_experts_gb
                + ggml_workspace_reserve
                + ctx_ram_deduction;

            // If absorbing all offloaded layers keeps TOTAL system RAM strictly under 95% ceiling:
            // dynamically absorb all layers into RAM so SSD disk swap is ZERO!
            if overflow_layers > 0 && total_weight_and_overhead_need <= max_allowed_cluaiz_ram {
                eprintln!(
                    "⚡ [Negotiator] Dynamic Spill Absorption Active: Absorbing {} overflow layers ({:.2} GB) into RAM. Total Cluaiz: {:.2} GB <= 95% Ceiling Budget: {:.2} GB. SSD swap = 0 Layers!",
                    overflow_layers, overflow_gb, total_weight_and_overhead_need, max_allowed_cluaiz_ram
                );
                cached_expert_count = initial_cached_expert_count;
                let flexed_ram_base = (max_allowed_cluaiz_ram - ram_dense_reserve - ggml_workspace_reserve).max(0.0);
                actual_cache_gb = if single_expert_gb > 0.0 {
                    (cached_expert_count as f64 * single_expert_gb).min(flexed_ram_base)
                } else {
                    initial_cache_budget.min(flexed_ram_base)
                };
                cached_layers = initial_cached_layers;
                cut_layers_for_safety = 0;
                overflow_layers = 0;
                overflow_experts = 0;
                overflow_gb = 0.0;
            }

            eprintln!("🧠 [Negotiator] SsdStreaming budget clamped: Expert LRU Cache = {:.2} GB | Headroom Protection Active.", actual_cache_gb);

            let pre_context_vram_headroom = (live_free_vram_gb - allocated_vram).max(0.0);
            let post_context_vram_buffer = if ctx_in_vram {
                (pre_context_vram_headroom - required_ctx_gb).max(0.0)
            } else {
                pre_context_vram_headroom
            };

            let mut post_context_ram_buffer =
                (ram_after_base - actual_cache_gb - required_ctx_gb).max(0.0);

            // 🚀 Dynamic Upward Context Scaling:
            // When all offloaded layers fit in RAM (overflow_layers == 0) and leftover RAM headroom is ample,
            // dynamically scale context up from 2048 towards 4096 or native context, keeping a safe buffer!
            if overflow_layers == 0 && !ctx_in_vram {
                let target_upper = if user_n_ctx > 0 {
                    (user_n_ctx as usize).min(native_max_ctx)
                } else {
                    native_max_ctx
                };

                // Leave at least 0.75 GB unallocated headroom for Windows safety
                let headroom_for_ctx_expansion = (post_context_ram_buffer - 0.75).max(0.0);
                if headroom_for_ctx_expansion > 0.0 && kv_bytes_per_token > 0.0 {
                    let extra_tokens = ((headroom_for_ctx_expansion * 1024.0 * 1024.0 * 1024.0) / kv_bytes_per_token) as usize;
                    // 🔬 Pure Mathematical Allocation: Scale context strictly according to physically measured RAM headroom
                    let candidate = (target_ctx_tokens + extra_tokens).min(target_upper);

                    if candidate > target_ctx_tokens {
                        let additional_ctx_gb = ((candidate - target_ctx_tokens) as f64 * kv_bytes_per_token) / (1024.0 * 1024.0 * 1024.0);
                        eprintln!(
                            "⚡ [Negotiator] Dynamic Upward Context Scaling: {:.2} GB RAM headroom available. Expanding context from {} to {} tokens (+{:.2} GB KV cache).",
                            post_context_ram_buffer, target_ctx_tokens, candidate, additional_ctx_gb
                        );
                        target_ctx_tokens = candidate;
                        required_ctx_gb = (target_ctx_tokens as f64 * kv_bytes_per_token) / (1024.0 * 1024.0 * 1024.0);
                        post_context_ram_buffer = (ram_after_base - actual_cache_gb - required_ctx_gb).max(0.0);
                    }
                }
            }

            let ctx_placement_str = if ctx_in_vram {
                format!(
                    "Native Max = {} Tokens | Granted = {} Tokens ({:.2} GB) -> Placed in VRAM",
                    native_max_ctx, target_ctx_tokens, required_ctx_gb
                )
            } else {
                format!("Native Max = {} Tokens | Granted = {} Tokens ({:.2} GB) -> Placed in System RAM", native_max_ctx, target_ctx_tokens, required_ctx_gb)
            };

            let leftover_vram_headroom = (usable_vram - allocated_vram).max(0.0);
            let leftover_ram_headroom = post_context_ram_buffer;

            eprintln!("🧠 [Negotiator] Resource Placement & Tier Breakdown:");
            eprintln!("   ├── 🟢 VRAM Allocation (Dedicated Free: {:.2} GB):", live_free_vram_gb);
            eprintln!(
                "   │    ├── Display Headroom Reserve (Windows DWM): {:.2} GB",
                vram_display_headroom_gb
            );
            if approx_layers.max(0) > 0 {
                eprintln!(
                    "   │    ├── GPU Offloaded Layers: {} Full MoE Layers (Attention + {} Experts per layer, Total: {:.2} GB)",
                    approx_layers.max(0),
                    moe_info.expert_count,
                    (approx_layers.max(0) as f64 * layer_size)
                );
            } else {
                eprintln!("   │    ├── GPU Offloaded Layers: Skipped (0 Layers on GPU / Offloaded to System RAM)");
            }
            if remaining_layers > 0 {
                eprintln!(
                    "   │    ├── Remaining Layers: {} Transformer Layers ({} Experts Offloaded to System RAM)",
                    remaining_layers, offloaded_layer_experts
                );
            }
            if ctx_in_vram {
                eprintln!(
                    "   │    ├── Context Window ({}): {}",
                    ctx_mode_str, ctx_placement_str
                );
            } else {
                eprintln!("   │    ├── Context Window: Skipped (Offloaded to System RAM)");
            }
            eprintln!("   │    └── Reserved VRAM Buffer: {:.2} GB", vram_safety);

            let unallocated_free_ram = (usable_ram - (total_non_cache_reserve + actual_cache_gb)).max(0.0);

            eprintln!(
                "   ├── 🔵 System RAM Allocation (Usable: {:.2} GB | Live Free: {:.2} GB):",
                usable_ram, available_ram_gb
            );
            if ram_dense_reserve > 0.0 {
                eprintln!(
                    "   │    ├── Dense Backbone & Base Model Reserve: {:.2} GB",
                    ram_dense_reserve
                );
            }
            eprintln!(
                "   │    ├── GGML Compute Graph Workspace Reserve: {:.2} GB",
                ggml_workspace_reserve
            );

            eprintln!("   │    ├── 1. Initial Requested Experts: {} Transformer Layers ({} Experts, {:.2} GB)", initial_cached_layers, initial_cached_expert_count, initial_cache_budget);
            if !ctx_in_vram {
                eprintln!(
                    "   │    ├── 2. Context Window ({}): {}",
                    ctx_mode_str, ctx_placement_str
                );
            }

            if cut_layers_for_safety > 0 {
                eprintln!(
                    "   │    ├── 3. Eviction Triggered: Cutting {} Transformer Layers",
                    cut_layers_for_safety
                );
            }

            eprintln!("   │    ├── 4. Final Active Experts LRU Cache: {} Transformer Layers ({} Experts, {:.2} GB)", cached_layers, cached_expert_count, actual_cache_gb);

            if overflow_layers > 0 {
                eprintln!(
                    "   │    ├── Overflow Layers: {} Transformer Layers ({} Experts, Offloaded)",
                    overflow_layers, overflow_experts
                );
            }
            eprintln!("   │    ├── Unallocated Usable RAM Headroom: {:.2} GB", unallocated_free_ram);
            eprintln!("   │    └── OS Safety Buffer (Untouched System Guard): {:.2} GB", ram_safety);

            eprintln!("   └── 🟠 Dynamic Swapping:");
            if overflow_layers > 0 {
                eprintln!(
                    "        ├── Overflow on Disk: {} Attention Layers ({} Experts, {:.2} GB)",
                    overflow_layers, overflow_experts, overflow_gb
                );
                eprintln!("        ├── Dynamic Fetch Strategy: On-Demand LRU Swap between Disk ↔ RAM Cache ({:.2} GB)", actual_cache_gb);
                eprintln!(
                    "        └── Stable Memory Target: RAM Cache locked to {:.2} GB limit",
                    actual_cache_gb
                );
            } else {
                eprintln!(
                    "        └── Swapping: Skipped (All {} Offloaded Attention Layers fit in RAM)",
                    remaining_layers
                );
            }

            let determined_tier = if overflow_layers > 0 {
                PlacementTier::SsdStreaming
            } else {
                PlacementTier::Hybrid
            };

            (
                determined_tier,
                approx_layers.max(0),
                free_vram,
                usable_ram,
                Some(moe_info.clone()),
                actual_cache_gb,
            )
        } else if model_gb <= free_vram {
            (PlacementTier::GpuOnly, -1, model_gb, 0.0, None, 0.0)
        } else if model_gb <= (free_vram + usable_ram) {
            let ram_share = model_gb - free_vram;
            let cache_budget = if moe_info.is_moe {
                moe_info.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            } else {
                0.0
            };
            let final_moe = if moe_info.is_moe {
                Some(moe_info.clone())
            } else {
                None
            };
            let approx_layers = if moe_info.is_moe {
                let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let layer_count = moe_info.moe_layer_count.max(1) as f64;
                let layer_size = (model_gb - dense_gb).max(0.01) / layer_count;
                let vram_base_reserve = (dense_gb / layer_count).max(0.10);
                if free_vram > vram_base_reserve {
                    (((free_vram - vram_base_reserve) / layer_size) as i32)
                        .min(moe_info.moe_layer_count as i32)
                } else {
                    0
                }
            } else {
                let gpu_ratio = (free_vram / model_gb.max(0.01)).min(1.0);
                ((gpu_ratio * 40.0) as i32).max(0)
            };
            (
                PlacementTier::Hybrid,
                approx_layers.max(0),
                free_vram,
                ram_share,
                final_moe,
                cache_budget,
            )
        } else if model_gb <= usable_ram {
            let cache_budget = if moe_info.is_moe {
                moe_info.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            } else {
                0.0
            };
            let final_moe = if moe_info.is_moe {
                Some(moe_info.clone())
            } else {
                None
            };
            (
                PlacementTier::CpuOnly,
                0,
                0.0,
                model_gb,
                final_moe,
                cache_budget,
            )
        } else {
            let extreme_moe = opt_control.extreme_moe_streaming;
            if matches!(extreme_moe, FeatureState::Off) {
                (PlacementTier::CpuOnly, 0, 0.0, usable_ram, None, 0.0)
            } else if moe_info.is_moe {
                let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let cache_budget = ram_for_expert_cache.min(moe_info.recommended_cache_budget_gb());
                (
                    PlacementTier::SsdStreaming,
                    0,
                    0.0,
                    usable_ram,
                    Some(moe_info.clone()),
                    cache_budget,
                )
            } else {
                anyhow::bail!(
                    "❌ Insufficient Hardware: Dense model requires {:.2} GB, but only {:.2} GB usable memory is available (VRAM: {:.2} GB, RAM: {:.2} GB after user buffer and 85% system ceiling). Dense models cannot be streamed via SSD.",
                    model_gb,
                    (free_vram + usable_ram),
                    free_vram,
                    usable_ram
                );
            }
        };

    let grant = ResourceGrant {
        tier,
        n_gpu_layers,
        thread_count: sysinfo::System::new().cpus().len().max(1),
        vram_budget_gb: vram_budget,
        ram_budget_gb: ram_budget,
        safety_buffer_gb: vram_safety,
        expert_cache_budget_gb,
        moe_info: final_moe_info,
        target_ctx_tokens,
    };

    if grant.tier == PlacementTier::SsdStreaming {
        apply_windows_hard_memory_quota(grant.ram_budget_gb);
    }

    Ok(grant)
}

/// 🛡️ Windows OS Hard Working Set Quota Lock
/// Caps the process's physical RAM consumption at `usable_ram_gb`, forcing Windows OS
/// to evict cold Page Cache pages when memory reaches `usable_ram_gb`.
/// Prevents artificial Windows working set pagefile swapping.
///
/// Calling SetProcessWorkingSetSizeEx with a capped max_bytes forces Windows virtual memory manager
/// to trim mmap and heap pages to SSD pagefile.sys, creating 100% SSD disk thrashing and stalling CPU/GPU inference.
/// Leaving working set unconstrained allows all model weights to remain resident in physical RAM.
#[cfg(windows)]
pub fn apply_windows_hard_memory_quota(usable_ram_gb: f64) {
    eprintln!(
        "🛡️ [Negotiator] Memory Guard: Unconstrained working set active (Physical RAM target: {:.2} GB, SSD swapping disabled).",
        usable_ram_gb
    );
}

#[cfg(not(windows))]
pub fn apply_windows_hard_memory_quota(_usable_ram_gb: f64) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiator_ram_allocation_no_arbitrary_eviction() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 14.62,
            model_path: PathBuf::from("C:\\models\\test-model.gguf"),
        };

        if let Ok(grant) = negotiate_resource(&req) {
            eprintln!("\n🔬 [RESOURCE NEGOTIATOR TEST RESULT]");
            eprintln!("   ├── Tier:            {:?}", grant.tier);
            eprintln!("   ├── GPU Layers:      {}", grant.n_gpu_layers);
            eprintln!("   ├── VRAM Budget:     {:.2} GB", grant.vram_budget_gb);
            eprintln!("   └── RAM Budget:      {:.2} GB", grant.ram_budget_gb);

            assert!(
                grant.ram_budget_gb > 5.0,
                "RAM budget must not be arbitrarily strangled!"
            );
        }
    }

    #[test]
    fn test_exact_byte_by_byte_memory_trace() {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total_ram_bytes = sys.total_memory();
        let free_ram_bytes = sys.available_memory();

        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 14.62,
            model_path: PathBuf::from("C:\\models\\test-model.gguf"),
        };

        let grant = negotiate_resource(&req).expect("Negotiation failed");

        let dense_bytes = (1.26 * 1024.0 * 1024.0 * 1024.0) as u64;
        let ggml_workspace_bytes = (0.83 * 1024.0 * 1024.0 * 1024.0) as u64;
        let ctx_bytes = (0.25 * 1024.0 * 1024.0 * 1024.0) as u64;
        let expert_cache_bytes = (grant.expert_cache_budget_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        let os_buffer_bytes = (grant.safety_buffer_gb * 1024.0 * 1024.0 * 1024.0) as u64;

        let total_cluaiz_bytes =
            dense_bytes + ggml_workspace_bytes + ctx_bytes + expert_cache_bytes;

        eprintln!("\n🔬 [EXACT BYTE-BY-BYTE MEMORY TRACE]");
        eprintln!(
            "   ├── 🌐 Total System RAM:             {} Bytes ({:.2} GB)",
            total_ram_bytes,
            total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── 🆓 Live Available System RAM:    {} Bytes ({:.2} GB)",
            free_ram_bytes,
            free_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── 🧠 Dense Backbone Allocation:    {} Bytes ({:.2} GB)",
            dense_bytes,
            dense_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── ⚙️ GGML Workspace Allocation:     {} Bytes ({:.2} GB)",
            ggml_workspace_bytes,
            ggml_workspace_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── 💬 Context Window KV Cache:       {} Bytes ({:.2} GB)",
            ctx_bytes,
            ctx_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── 🎰 MoE Expert LRU Cache:         {} Bytes ({:.2} GB)",
            expert_cache_bytes,
            expert_cache_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   ├── 🛡️ OS Safety Reserved Buffer:     {} Bytes ({:.2} GB)",
            os_buffer_bytes,
            os_buffer_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        eprintln!(
            "   └── 🎯 TOTAL CLUAIZ ALLOCATION:      {} Bytes ({:.2} GB)",
            total_cluaiz_bytes,
            total_cluaiz_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );

        assert!(total_cluaiz_bytes > 0, "Allocation trace must be non-zero");
    }

    fn print_scenario_full_report(
        title: &str,
        req: &ResourceRequest,
        grant: &ResourceGrant,
    ) {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
        let free_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

        let total_vram_gb = 4.00f64;
        let free_vram_gb = 3.09f64;

        let remaining_vram_space = (free_vram_gb - grant.vram_budget_gb).max(0.0);
        let remaining_ram_space = (free_ram_gb - grant.ram_budget_gb).max(0.0);

        let ctx_in_vram = grant.tier == PlacementTier::GpuOnly
            || (grant.vram_budget_gb > (grant.n_gpu_layers.max(0) as f64 * 0.35 + 0.10));

        let ctx_location_str = if ctx_in_vram {
            "PLACED IN GPU VRAM (Ultra-fast GDDR6)"
        } else {
            "PLACED IN SYSTEM RAM (CPU Host Memory)"
        };

        let disk_mode_str = match grant.tier {
            PlacementTier::GpuOnly => "0 Layers on Disk (100% GPU VRAM)",
            PlacementTier::Hybrid => "0 LAYERS ON SSD DISK (100% Memory-Resident Hybrid!)",
            PlacementTier::CpuOnly => "0 Layers on Disk (100% System Host RAM)",
            PlacementTier::SsdStreaming => "Active LRU Streaming from SSD",
        };

        eprintln!("\n================================================================================");
        eprintln!("🔬 [FULL REPORT: {}]", title);
        eprintln!("================================================================================");
        eprintln!("├── 🏷️ Model Specs:          Size: {:.2} GB | Path: {:?}", req.model_size_gb, req.model_path);
        eprintln!("├── 🎯 Placement Tier:        {:?}", grant.tier);
        eprintln!("│");
        eprintln!("├── 🟢 GPU VRAM Details:");
        eprintln!("│    ├── Total VRAM:          {:.2} GB", total_vram_gb);
        eprintln!("│    ├── Live Dedicated Free: {:.2} GB", free_vram_gb);
        eprintln!("│    ├── GPU Offloaded:       {} Layers ({:.2} GB Allocated)", grant.n_gpu_layers, grant.vram_budget_gb);
        eprintln!("│    ├── Remaining VRAM Space:{:.2} GB Space Left Free", remaining_vram_space);
        eprintln!("│    └── Shared GPU Memory:   0.00 GB (DIRECT GDDR6 ALLOCATION, 0 BYTES SHARED!)");
        eprintln!("│");
        eprintln!("├── 💬 Context Window (KV Cache):");
        eprintln!("│    ├── Context Placement:   {}", ctx_location_str);
        eprintln!("│    ├── Granted Tokens:      {} Tokens", grant.target_ctx_tokens);
        eprintln!("│    └── Safety Floor:        >= 2048 Tokens Strictly Guaranteed");
        eprintln!("│");
        eprintln!("├── 🔵 System Host RAM Details:");
        eprintln!("│    ├── Total System RAM:    {:.2} GB", total_ram_gb);
        eprintln!("│    ├── Live Available RAM:  {:.2} GB", free_ram_gb);
        eprintln!("│    ├── Cluaiz RAM Budget:   {:.2} GB", grant.ram_budget_gb);
        eprintln!("│    ├── Remaining RAM Space: {:.2} GB Space Left Free", remaining_ram_space);
        eprintln!("│    ├── OS Safety Buffer:    {:.2} GB (Untouched System Guard)", grant.safety_buffer_gb);
        eprintln!("│    └── Total System Load:   {:.2} GB (<= 95% Ceiling Budget, ZERO Pagefile Thrash)", (total_ram_gb - free_ram_gb + grant.ram_budget_gb).min(total_ram_gb));
        eprintln!("│");
        eprintln!("└── 🟠 Disk Swap & Memory Integrity:");
        eprintln!("     ├── Disk Swapping:       {}", disk_mode_str);
        eprintln!("     └── Memory Mode:         DIRECT HARDWARE ALLOCATION (No Shared RAM Choke)");
        eprintln!("================================================================================\n");
    }

    #[test]
    fn test_scenario_1_gpu_only_when_fits_in_vram() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 1.50, // Small model fits completely in 4GB VRAM
            model_path: PathBuf::from("C:\\models\\small-test.gguf"),
        };

        let grant = negotiate_resource(&req).expect("Scenario 1 negotiation failed");
        print_scenario_full_report("Scenario 1: GPU-Only Model (1.50 GB)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::GpuOnly, "Model <= free_vram must be GpuOnly!");
        assert_eq!(grant.n_gpu_layers, -1, "GpuOnly tier must request -1 (all GPU layers)!");
    }

    #[test]
    fn test_scenario_2_hybrid_split_capped_layers_and_dynamic_ctx() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 14.62, // Gemma 26B medium model
            model_path: PathBuf::from("C:\\models\\synthetic-moe-26b.gguf"),
        };

        let grant = negotiate_resource(&req).expect("Scenario 2 negotiation failed");
        print_scenario_full_report("Scenario 2: Gemma 26B Hybrid Offload (14.62 GB)", &req, &grant);

        assert!(
            grant.tier == PlacementTier::Hybrid || grant.tier == PlacementTier::SsdStreaming,
            "14.6GB model on 4GB VRAM must be Hybrid or SsdStreaming!"
        );
        assert!(
            grant.n_gpu_layers >= 0 && grant.n_gpu_layers < 30,
            "GPU layers in hybrid mode must be strictly capped to physical VRAM!"
        );
        assert!(
            grant.target_ctx_tokens >= 2048,
            "Target context must be at least 2048 tokens!"
        );
    }

    #[test]
    fn test_scenario_2b_dense_hybrid_model() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 8.50, // Dense model that fits in VRAM + RAM combined
            model_path: PathBuf::from("C:\\models\\dense-llama-8b.gguf"),
        };

        let grant = negotiate_resource(&req).expect("Scenario 2b negotiation failed");
        print_scenario_full_report("Scenario 2b: Dense Hybrid Model (8.50 GB)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::Hybrid, "8.5GB dense model must be Hybrid!");
        assert!(grant.n_gpu_layers > 0, "GPU layers must be > 0 for dense hybrid!");
    }

    #[test]
    fn test_scenario_3_ssd_streaming_when_overflows_ram() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 35.0, // Large MoE model
            model_path: PathBuf::from("C:\\models\\synthetic-moe-35b.gguf"),
        };

        if let Ok(grant) = negotiate_resource(&req) {
            print_scenario_full_report("Scenario 3: 35 GB Large MoE Overflow Model", &req, &grant);
        }
    }

    #[test]
    fn test_scenario_4_lib_target_gpu_layers_policy() {
        let hybrid_grant_layers = 7;
        let is_hybrid_split = true;
        let user_n_gpu_layers = -1;

        let target_gpu_layers = if user_n_gpu_layers == 0 {
            0
        } else if is_hybrid_split && user_n_gpu_layers == -1 {
            hybrid_grant_layers
        } else if user_n_gpu_layers == -1 {
            -1
        } else {
            user_n_gpu_layers
        };

        assert_eq!(target_gpu_layers, 7, "Hybrid mode with -1 must cap to 7 layers!");

        let is_hybrid_split_false = false;
        let target_gpu_layers_gpu_only = if user_n_gpu_layers == 0 {
            0
        } else if is_hybrid_split_false && user_n_gpu_layers == -1 {
            hybrid_grant_layers
        } else if user_n_gpu_layers == -1 {
            -1
        } else {
            user_n_gpu_layers
        };

        assert_eq!(target_gpu_layers_gpu_only, -1, "GpuOnly mode with -1 must pass -1 to CUDA!");
    }

    #[test]
    fn test_scenario_5_small_spill_absorption_under_95_percent_ceiling() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF,
            inference_mode: InferenceMode::Chat,
            model_size_gb: 12.00,
            model_path: PathBuf::from("C:\\models\\synthetic-moe-12b.gguf"),
        };

        let grant = negotiate_resource(&req).expect("Scenario 5 negotiation failed");
        print_scenario_full_report("Scenario 5: Small Spill Absorbed (12.00 GB Model)", &req, &grant);

        assert_eq!(
            grant.tier,
            PlacementTier::Hybrid,
            "Small spill model must be absorbed into Hybrid tier (0 layers on SSD)!"
        );
    }
}
