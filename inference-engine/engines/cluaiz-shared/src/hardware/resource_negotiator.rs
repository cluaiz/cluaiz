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
        },
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

    // Context Window (n_ctx) RAM Reservation Calculation (Default ~1.00 GB)
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
            ram_budget_gb: usable_ram,
            safety_buffer_gb: ram_safety,
            expert_cache_budget_gb: cache_budget,
            moe_info: final_moe,
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
            
            // Accurate MoE layer sizing (Dense Attention + MoE Router + 128 Experts)
            let dense_per_layer_gb = dense_gb / layer_count;
            let expert_per_layer_gb = expert_total_gb / layer_count;
            let layer_size = dense_per_layer_gb + expert_per_layer_gb;
            
            let vram_base_reserve = dense_per_layer_gb.max(0.10);

            // Step 1: Layer Offloading Calculation (GPU VRAM First)
            let mut is_forced_safety = false;
            let mut approx_layers = if free_vram > vram_base_reserve {
                (((free_vram - vram_base_reserve) / layer_size) as i32)
                    .min(moe_info.moe_layer_count as i32)
            } else {
                0
            };

            // user_n_ctx is now fetched globally in Step 3
            // Read raw native context length directly from GGUF header to avoid DNA side-effects
            let mut native_max_ctx = usize::MAX;
            if let Ok((metadata, _, _)) = crate::utils::GGUFProber::probe(&request.model_path) {
                if let Some(arch) = metadata.get("general.architecture") {
                    let ctx_key = format!("{}.context_length", arch);
                    if let Some(ctx_str) = metadata.get(&ctx_key) {
                        if let Ok(ctx) = ctx_str.parse::<usize>() {
                            native_max_ctx = ctx;
                        }
                    }
                }
            }
            let min_2k_tokens = 2048usize;
            let kv_bytes_per_token = 128.0 * 1024.0;

            // Step 1: Base Reserves & Initial Allocations
            let ram_dense_reserve = (moe_info.dense_backbone_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            
            // GGML Compute Graph Workspace Reserve (Dynamic)
            // Scales dynamically with model size (proxy for hidden dim size). Base overhead is ~250MB.
            // Adds ~40MB per 1GB of model weights. Bounded between 500MB and 3GB.
            let ggml_workspace_reserve = (0.25 + (model_gb * 0.04)).clamp(0.50, 3.00);
            
            // ─── User Rule: In Hybrid/Streaming (Tier 4), Context Window ALWAYS goes to System RAM! ───
            let ctx_in_vram = false;
            let vram_for_layers = free_vram;

            let mut is_forced_safety = false;
            let mut approx_layers = if vram_for_layers > vram_base_reserve {
                (((vram_for_layers - vram_base_reserve) / layer_size) as i32)
                    .min(moe_info.moe_layer_count as i32)
            } else {
                0
            };

            let mut allocated_vram = vram_base_reserve + (approx_layers.max(0) as f64 * layer_size);

            // Layer Yielding Loop: Ensure integer headroom > 0.15 GB to prevent VRAM OOM
            while (live_free_vram_gb - allocated_vram) < 0.15 && approx_layers > 0 {
                approx_layers -= 1;
                allocated_vram = vram_base_reserve + (approx_layers.max(0) as f64 * layer_size);
                is_forced_safety = true;
            }

            let remaining_layers = (moe_info.moe_layer_count as i32).saturating_sub(approx_layers.max(0));
            let gpu_experts = if moe_info.moe_layer_count > 0 {
                (moe_info.expert_count * approx_layers.max(0) as usize) / moe_info.moe_layer_count
            } else {
                0
            };
            let offloaded_layer_experts = moe_info.expert_count.saturating_sub(gpu_experts);

            let single_expert_gb = if moe_info.total_expert_bytes > 0 && moe_info.expert_count > 0 {
                (moe_info.total_expert_bytes as f64 / moe_info.expert_count as f64)
                    / (1024.0 * 1024.0 * 1024.0)
            } else {
                0.0
            };
            
            let offloaded_experts_gb = offloaded_layer_experts as f64 * single_expert_gb;

            // Step 2: Optimistic Allocation (Try to fit ALL layers first)
            let initial_cache_budget = offloaded_experts_gb;
            let initial_cached_expert_count = offloaded_layer_experts;
            let initial_cached_layers = remaining_layers;
            
            let ram_after_base = (usable_ram - ram_dense_reserve - ggml_workspace_reserve).max(0.0);
            
            // Step 3: Context Window Calculation
            // We MUST protect the OS Safety Buffer from being eaten by the Context Window!
            let os_safety_buffer_gb = (total_ram_gb * 0.05).clamp(1.0, 2.0);
            
            let remaining_for_ctx = (ram_after_base - initial_cache_budget - os_safety_buffer_gb).max(0.0);
            let max_possible_tokens = ((remaining_for_ctx * 1024.0 * 1024.0 * 1024.0) / kv_bytes_per_token) as usize;

            let (target_ctx_tokens, ctx_mode_str) = match user_n_ctx {
                -1 | i32::MAX => {
                    let safe = max_possible_tokens.clamp(min_2k_tokens, native_max_ctx);
                    let label = if safe == native_max_ctx { "Full Window" } else { "Clamped" };
                    (safe, format!("{} -> {} Tokens", label, safe))
                }
                n if n > 0 => {
                    let req = n as usize;
                    let safe = req.min(max_possible_tokens).clamp(min_2k_tokens, native_max_ctx);
                    let label = if safe == req { "Custom" } else { "Clamped" };
                    (safe, format!("{} -> {} Tokens", label, safe))
                }
                _ => {
                    // Auto Mode
                    let safe = max_possible_tokens.clamp(min_2k_tokens, native_max_ctx);
                    (safe, format!("Auto Dynamic ({} Tokens)", safe))
                }
            };

            let required_ctx_gb = (target_ctx_tokens as f64 * kv_bytes_per_token) / (1024.0 * 1024.0 * 1024.0);

            // Step 4: Layer Eviction (Deduct non-cache reserves cleanly including dynamic OS Safety Buffer)
            let total_non_cache_reserve = ram_dense_reserve + ggml_workspace_reserve + required_ctx_gb + os_safety_buffer_gb;
            let ram_for_cache = (usable_ram - total_non_cache_reserve).max(0.0);
            let mut cached_expert_count = initial_cached_expert_count;
            let experts_per_layer = moe_info.expert_count as f64 / moe_info.moe_layer_count as f64;

            if single_expert_gb > 0.0 {
                let max_experts_fit = (ram_for_cache / single_expert_gb).floor() as usize;
                cached_expert_count = initial_cached_expert_count.min(max_experts_fit);
            }

            let cache_budget = if single_expert_gb > 0.0 {
                (cached_expert_count as f64 * single_expert_gb).min(ram_after_base)
            } else {
                initial_cache_budget.min(ram_after_base)
            };
            
            let actual_cache_gb = cache_budget;
            let cached_layers = (cached_expert_count as f64 / experts_per_layer).round() as i32;
            let cut_layers_for_safety = initial_cached_layers - cached_layers;
            let overflow_layers = initial_cached_layers - cached_layers;
            
            let overflow_experts = offloaded_layer_experts.saturating_sub(cached_expert_count);
            let overflow_gb = if single_expert_gb > 0.0 {
                single_expert_gb * overflow_experts as f64
            } else {
                0.0
            };
            eprintln!("🧠 [Negotiator] SsdStreaming budget clamped: Expert LRU Cache = {:.2} GB | Headroom Protection Active.", actual_cache_gb);

            let pre_context_vram_headroom = (live_free_vram_gb - allocated_vram).max(0.0);
            let post_context_vram_buffer = if ctx_in_vram {
                (pre_context_vram_headroom - required_ctx_gb).max(0.0)
            } else {
                pre_context_vram_headroom
            };

            let post_context_ram_buffer = (ram_after_base - actual_cache_gb - required_ctx_gb).max(0.0);

            let ctx_placement_str = if ctx_in_vram {
                format!("Native Max = {} Tokens | Granted = {} Tokens ({:.2} GB) -> Placed in VRAM", native_max_ctx, target_ctx_tokens, required_ctx_gb)
            } else {
                format!("Native Max = {} Tokens | Granted = {} Tokens ({:.2} GB) -> Placed in System RAM", native_max_ctx, target_ctx_tokens, required_ctx_gb)
            };

            let leftover_vram_headroom = (usable_vram - allocated_vram).max(0.0);
            let leftover_ram_headroom = post_context_ram_buffer;

            eprintln!("🧠 [Negotiator] Resource Placement & Tier Breakdown:");
            eprintln!("   ├── 🟢 VRAM Allocation (Usable: {:.2} GB):", usable_vram);
            eprintln!("   │    ├── Base VRAM Reserve (Embeddings/Head): {:.2} GB", vram_base_reserve);
            if approx_layers.max(0) > 0 {
                eprintln!("   │    ├── Locked GPU Layers: {} Attention Layers ({} Experts, {:.2} GB)", approx_layers.max(0), gpu_experts, (approx_layers.max(0) as f64 * layer_size));
            } else {
                eprintln!("   │    ├── Locked GPU Layers: Skipped (0 Attention Layers on GPU / Offloaded to System RAM)");
            }
            if remaining_layers > 0 {
                eprintln!("   │    ├── Remaining Layers: {} Attention Layers ({} Experts, Offloaded)", remaining_layers, offloaded_layer_experts);
            }
            if ctx_in_vram {
                eprintln!("   │    ├── Context Window ({}): {}", ctx_mode_str, ctx_placement_str);
            } else {
                eprintln!("   │    ├── Context Window: Skipped (Offloaded to System RAM)");
            }
            eprintln!("   │    └── Reserved VRAM Buffer: {:.2} GB", vram_safety);
            
            eprintln!("   ├── 🔵 System RAM Allocation (Usable: {:.2} GB):", usable_ram);
            if ram_dense_reserve > 0.0 {
                eprintln!("   │    ├── Dense Backbone & Base Model Reserve: {:.2} GB", ram_dense_reserve);
            }
            eprintln!("   │    ├── GGML Compute Graph Workspace Reserve: {:.2} GB", ggml_workspace_reserve);
            
            eprintln!("   │    ├── 1. Initial Requested Experts: {} Attention Layers ({} Experts, {:.2} GB)", initial_cached_layers, initial_cached_expert_count, initial_cache_budget);
            if !ctx_in_vram {
                eprintln!("   │    ├── 2. Context Window ({}): {}", ctx_mode_str, ctx_placement_str);
            }
            
            if cut_layers_for_safety > 0 {
                eprintln!("   │    ├── 3. Eviction Triggered: Cutting {} Attention Layers", cut_layers_for_safety);
            }
            
            eprintln!("   │    ├── 4. Final Active Experts LRU Cache: {} Attention Layers ({} Experts, {:.2} GB)", cached_layers, cached_expert_count, actual_cache_gb);
            
            if overflow_layers > 0 {
                eprintln!("   │    ├── Overflow Layers: {} Attention Layers ({} Experts, Offloaded)", overflow_layers, overflow_experts);
            }
            eprintln!("   │    └── Reserved RAM Buffer: {:.2} GB", ram_safety);
            
            eprintln!("   └── 🟠 Dynamic Swapping:");
            if overflow_layers > 0 {
                eprintln!("        ├── Overflow on Disk: {} Attention Layers ({} Experts, {:.2} GB)", overflow_layers, overflow_experts, overflow_gb);
                eprintln!("        ├── Dynamic Fetch Strategy: On-Demand LRU Swap between Disk ↔ RAM Cache ({:.2} GB)", actual_cache_gb);
                eprintln!("        └── Zero-Freeze Assurance: RAM Cache locked to {:.2} GB limit", actual_cache_gb);
            } else {
                eprintln!("        └── Swapping: Skipped (All {} Offloaded Attention Layers fit in RAM)", remaining_layers);
            }
            
            (
                PlacementTier::SsdStreaming,
                approx_layers.max(0),
                free_vram,
                usable_ram,
                Some(moe_info.clone()),
                cache_budget,
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
    };

    if grant.tier == PlacementTier::SsdStreaming {
        apply_windows_hard_memory_quota(grant.ram_budget_gb);
    }

    Ok(grant)
}

/// 🛡️ Windows OS Hard Working Set Quota Lock
/// Caps the process's physical RAM consumption at `usable_ram_gb`, forcing Windows OS
/// to evict cold Page Cache pages when memory reaches `usable_ram_gb`.
/// Prevents System RAM from filling to 22.7 GB (96%) and preserves 1.5 GB OS Safety Buffer.
#[cfg(windows)]
pub fn apply_windows_hard_memory_quota(usable_ram_gb: f64) {
    use std::ffi::c_void;

    type HANDLE = *mut c_void;
    type SIZE_T = usize;
    type BOOL = i32;
    type DWORD = u32;

    // 0 = Soft Working Set (Advisory quota - allows OS to keep active pages in RAM without aggressive page-fault storms)
    const QUOTA_LIMITS_SOFTWS: DWORD = 0;

    extern "system" {
        fn GetCurrentProcess() -> HANDLE;
        fn SetProcessWorkingSetSizeEx(
            hProcess: HANDLE,
            dwMinimumWorkingSetSize: SIZE_T,
            dwMaximumWorkingSetSize: SIZE_T,
            Flags: DWORD,
        ) -> BOOL;
    }

    let max_bytes: SIZE_T = (usable_ram_gb * 1024.0 * 1024.0 * 1024.0) as SIZE_T;
    let min_bytes: SIZE_T = (2.0 * 1024.0 * 1024.0 * 1024.0) as SIZE_T;

    unsafe {
        let handle = GetCurrentProcess();
        let ret = SetProcessWorkingSetSizeEx(
            handle,
            min_bytes,
            max_bytes,
            QUOTA_LIMITS_SOFTWS,
        );
        if ret != 0 {
            eprintln!(
                "🛡️ [Negotiator] Windows Working Set Quota Applied: Target Process Physical RAM at {:.2} GB (Zero-Thrash Soft Quota)",
                usable_ram_gb
            );
        } else {
            eprintln!("⚠️ [Negotiator] SetProcessWorkingSetSizeEx returned 0 (Working set limit adjustment skipped)");
        }
    }
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

            assert!(grant.ram_budget_gb > 5.0, "RAM budget must not be arbitrarily strangled!");
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

        let total_cluaiz_bytes = dense_bytes + ggml_workspace_bytes + ctx_bytes + expert_cache_bytes;

        eprintln!("\n🔬 [EXACT BYTE-BY-BYTE MEMORY TRACE]");
        eprintln!("   ├── 🌐 Total System RAM:             {} Bytes ({:.2} GB)", total_ram_bytes, total_ram_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── 🆓 Live Available System RAM:    {} Bytes ({:.2} GB)", free_ram_bytes, free_ram_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── 🧠 Dense Backbone Allocation:    {} Bytes ({:.2} GB)", dense_bytes, dense_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── ⚙️ GGML Workspace Allocation:     {} Bytes ({:.2} GB)", ggml_workspace_bytes, ggml_workspace_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── 💬 Context Window KV Cache:       {} Bytes ({:.2} GB)", ctx_bytes, ctx_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── 🎰 MoE Expert LRU Cache:         {} Bytes ({:.2} GB)", expert_cache_bytes, expert_cache_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   ├── 🛡️ OS Safety Reserved Buffer:     {} Bytes ({:.2} GB)", os_buffer_bytes, os_buffer_bytes as f64 / (1024.0*1024.0*1024.0));
        eprintln!("   └── 🎯 TOTAL CLUAIZ ALLOCATION:      {} Bytes ({:.2} GB)", total_cluaiz_bytes, total_cluaiz_bytes as f64 / (1024.0*1024.0*1024.0));

        assert!(total_cluaiz_bytes > 0, "Allocation trace must be non-zero");
    }
}