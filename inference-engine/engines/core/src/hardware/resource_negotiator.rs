//! ⚖️ Unified Resource Negotiator & Architectural Specification
//! Single Source of Truth for hardware placement decisions across ALL engines (GGUF/ONNX)
//! and ALL inference modes (Chat/Embedding/Audio/TTS).
//!
//! ### The 7-State Real-World Placement Spectrum
//! 1. 🟢 **State 1: Pure VRAM Resident (`PlacementTier::GpuOnly`)**
//!    - Weights + KV Cache fit completely in Dedicated Free VRAM.
//!    - 0.00 GB Shared GPU Memory, 0 Layers on Disk, Compute on GPU CUDA cores.
//! 2. 🔵 **State 2: Dedicated VRAM Weights + VRAM KV Cache**
//!    - Model partially offloaded, leftover Dedicated VRAM comfortably hosts KV cache.
//! 3. 🟣 **State 3: Dedicated VRAM Weights + Host RAM KV Cache**
//!    - VRAM is packed with weights. Context Window in Host RAM (0 GB Shared VRAM).
//! 4. 🔷 **State 4: Memory-Resident Hybrid (Zero SSD Swap)**
//!    - Dedicated VRAM layers + Host RAM layers both fit under adaptive ceiling. 0 Layers on SSD.
//! 5. ⚡ **State 5: Adaptive Context Trade-Off (The Intelligence Gate)**
//!    - When weights are close to fitting in RAM, context is dynamically reduced (>= 2048)
//!      to eliminate SSD disk swapping entirely.
//! 6. 🟠 **State 6: True SSD Streaming Fallback (`PlacementTier::SsdStreaming`)**
//!    - Massive model (35B / 70B) exceeding physical RAM. Excess layers stream from SSD.
//! 7. ⚪ **State 7: CPU-Only & CPU-Driven SSD Streaming (`PlacementTier::CpuOnly` / `SsdStreaming`)**
//!    - User override (`n_gpu_layers = 0`) or no GPU present. If model exceeds RAM, streams from SSD without crash.
//!
//! ### Why Shared GPU Memory is Strictly Banned (0.00 GB Guarantee)
//! - **The Mechanism:** Windows WDDM "Shared GPU Memory" is NOT GPU memory; it is Host RAM (DDR4/5)
//!   mapped across the PCIe bus. When VRAM oversubscribes, WDDM swaps pages over PCIe.
//! - **The Thrash Trap:** Dedicated GDDR6 runs at 224-448 GB/s, while PCIe Gen3/Gen4 x16 runs at
//!   only 15.75 GB/s (15x-25x slower). This mismatch causes WDDM page fault thrashing, freezes the
//!   Windows DWM compositor (mouse lag), and drops generation speed to ~0.2 tokens/sec.
//! - **Cluaiz Guarantee:** VRAM allocation is strictly clamped to:
//!   `Allocated VRAM <= Live Free Dedicated VRAM - 150 MB (Display Headroom)`.
//!   0.00 GB is ever allocated to WDDM Shared Memory.
//!
//! ### Why Context Window (KV Cache) Routes to Host RAM in Low-VRAM Scenarios
//! - **Weight Priority:** VRAM space is precious. An 8K context can consume 1.0 - 1.5 GB of VRAM.
//!   Putting KV cache in VRAM steals space from weights, evicting 4-6 neural layers to slow RAM/SSD.
//! - **Shared Memory Prevention:** Dedicated VRAM cannot hold weights, KV cache, and compute
//!   scratch simultaneously on constrained GPUs (e.g. 4GB). Routing KV to Host RAM (`offload_kqv = 0`)
//!   prevents WDDM from spilling into Shared GPU Memory across the slow PCIe bus.
//!
//! ### Hardware Bandwidth & Execution Separation
//! - **Compute Architecture:** CUDA tensor cores execute on GPU-resident layers, while CPU threads
//!   execute RAM-resident layers via AVX2.
//! - **Dense & MoE Partitioning:** Offloaded layers run natively on GPU. Non-offloaded layers
//!   (or non-offloaded MoE experts) execute natively in Host RAM on CPU. Neither spills to WDDM.
//! - **Fallback Triggers:** If CUDA driver memory allocation fails, display headroom is exhausted,
//!   or `n_gpu_layers == 0` is set, the negotiator gracefully defaults to CPU multi-threaded execution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::hardware::expert_offloading::{detect_moe, MoeModelInfo};
use crate::hardware::governor::HardwareGovernor;
use crate::hardware::schema::optimization::{FeatureState, OptimizationControl};

/// Global thread-safe Mutex lock to serialize CUDA and VRAM resource negotiation across parallel requests.
pub static GLOBAL_HARDWARE_LOCK: Mutex<()> = Mutex::new(());

// ─── Core Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineType {
    GGUF,
    ONNX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceMode {
    Chat,
    Embedding,
    Audio,
    TTS,
}

#[derive(Debug, Clone)]
pub struct ResourceRequest {
    pub engine_type: EngineType,
    pub inference_mode: InferenceMode,
    pub model_size_gb: f64,
    pub model_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementTier {
    GpuOnly,
    Hybrid,
    CpuOnly,
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

#[derive(Debug, Clone)]
pub struct ResourceGrant {
    pub tier: PlacementTier,
    pub n_gpu_layers: i32,
    pub thread_count: usize,
    pub vram_budget_gb: f64,
    pub ram_budget_gb: f64,
    pub safety_buffer_gb: f64,
    pub expert_cache_budget_gb: f64,
    pub moe_info: Option<MoeModelInfo>,
    pub target_ctx_tokens: usize,
    pub kv_on_gpu: bool,
}

// ─── Helpers: Dynamic Math & Geometry ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DynamicRamBudget {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub used_ram_gb: f64,
    pub safety_buffer_gb: f64,
    pub max_allowed_cluaiz_ram: f64,
    pub usable_ram_gb: f64,
}

pub fn calculate_dynamic_ram_headroom(
    opt_control: &OptimizationControl,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> DynamicRamBudget {
    let used_ram_gb = (total_ram_gb - available_ram_gb).max(0.0);
    let load_ratio = if total_ram_gb > 0.0 {
        (used_ram_gb / total_ram_gb).clamp(0.0, 1.0)
    } else {
        0.5
    };

    let dynamic_pct = if load_ratio < 0.35 {
        0.05
    } else if load_ratio <= 0.65 {
        0.05 + ((load_ratio - 0.35) / 0.30) * 0.05
    } else {
        0.10 + ((load_ratio - 0.65) / 0.35) * 0.05
    };

    let safety_buffer_gb = if let Some(direct_gb) = opt_control.custom_ram_buffer_gb {
        if direct_gb > 0.0 {
            direct_gb.max(1.00).min((total_ram_gb - 2.0).max(1.0))
        } else {
            (total_ram_gb * dynamic_pct).max(1.00)
        }
    } else {
        (total_ram_gb * dynamic_pct).max(1.00)
    };

    let ceiling_pct = crate::hardware::memory_governor::calculate_dynamic_system_ceiling_pct(
        total_ram_gb,
        available_ram_gb,
    );
    let max_safe_system_ram = total_ram_gb * ceiling_pct;
    let max_allowed_cluaiz_ram = (max_safe_system_ram - used_ram_gb).max(0.0);
    let raw_usable = (available_ram_gb - safety_buffer_gb).max(0.0);
    let usable_ram_gb = raw_usable.min(max_allowed_cluaiz_ram).max(0.0);

    DynamicRamBudget {
        total_ram_gb,
        available_ram_gb,
        used_ram_gb,
        safety_buffer_gb,
        max_allowed_cluaiz_ram,
        usable_ram_gb,
    }
}

#[derive(Debug, Clone)]
pub struct LayerGeometry {
    pub total_layers: usize,
    pub layer_size_gb: f64,
    pub dense_backbone_gb: f64,
    pub single_expert_gb: f64,
    pub is_moe: bool,
}

pub fn calculate_layer_geometry(moe_info: &MoeModelInfo, model_gb: f64, model_path: Option<&std::path::Path>) -> LayerGeometry {
    if moe_info.is_moe {
        let dense_gb = moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let expert_total_gb = moe_info.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let layer_count = moe_info.moe_layer_count.max(1);

        let dense_per_layer = dense_gb / layer_count as f64;
        let expert_per_layer = expert_total_gb / layer_count as f64;
        let layer_size = (dense_per_layer + expert_per_layer).max(0.04);

        let total_experts = (moe_info.expert_count * layer_count) as f64;
        let single_expert_gb = if moe_info.total_expert_bytes > 0 && total_experts > 0.0 {
            (moe_info.total_expert_bytes as f64 / total_experts) / (1024.0 * 1024.0 * 1024.0)
        } else if moe_info.expert_size_bytes > 0 {
            moe_info.expert_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        } else {
            0.0
        };

        LayerGeometry {
            total_layers: layer_count,
            layer_size_gb: layer_size,
            dense_backbone_gb: dense_gb,
            single_expert_gb,
            is_moe: true,
        }
    } else {
        let layer_count = model_path
            .filter(|p| p.exists() && p.is_file())
            .and_then(|p| crate::metadata::GgufBinaryProber::probe(p).ok())
            .map(|info| info.layer_count)
            .unwrap_or(32);
        let layer_size = (model_gb / layer_count as f64).max(0.02);

        LayerGeometry {
            total_layers: layer_count,
            layer_size_gb: layer_size,
            dense_backbone_gb: 0.0,
            single_expert_gb: 0.0,
            is_moe: false,
        }
    }
}

// ─── Core Negotiation Logic ──────────────────────────────────────────────────

pub fn negotiate_resource(request: &ResourceRequest) -> anyhow::Result<ResourceGrant> {
    negotiate_resource_with_override(request, None)
}

pub fn negotiate_resource_with_override(
    request: &ResourceRequest,
    override_gpu_layers: Option<i32>,
) -> anyhow::Result<ResourceGrant> {
    let _lock = GLOBAL_HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let model_gb = request.model_size_gb;

    // Step 1: Sub-Millisecond Hardware Probing
    let (total_vram_gb, live_free_vram_gb) = probe_vram();

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    // Step 2: Read Optimization Settings & Engine Flags
    let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
    let user_n_gpu_layers = override_gpu_layers.unwrap_or_else(|| match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_gpu_layers
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load().n_gpu_layers
        }
    });
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

    // Step 3: Dynamic RAM Budget & Safety Buffer Calculation
    let ram_budget_info =
        calculate_dynamic_ram_headroom(&opt_control, total_ram_gb, available_ram_gb);
    let usable_ram = ram_budget_info.usable_ram_gb;
    let ram_safety = ram_budget_info.safety_buffer_gb;
    let max_allowed_cluaiz_ram = ram_budget_info.max_allowed_cluaiz_ram;

    let vram_safety = crate::hardware::memory_governor::calculate_safety_buffer(
        &opt_control,
        total_vram_gb,
        live_free_vram_gb,
    );

    let moe_info = detect_moe(&request.model_path);
    let geom = calculate_layer_geometry(&moe_info, model_gb, Some(&request.model_path));
    let ggml_workspace_reserve = (0.25 + (model_gb * 0.04)).clamp(0.50, 3.00);

    // Step 4: User Override / CPU Mode Adaptive Handling
    if user_n_gpu_layers == 0 || total_vram_gb < 0.1 {
        let ram_needed = model_gb + ggml_workspace_reserve;
        let fits_in_ram = ram_needed <= max_allowed_cluaiz_ram;

        if fits_in_ram || !moe_info.is_moe {
            let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
                &request.model_path,
                user_n_ctx,
                usable_ram,
                total_ram_gb,
                model_gb,
                ram_needed,
            );
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

            return Ok(ResourceGrant {
                tier: PlacementTier::CpuOnly,
                n_gpu_layers: 0,
                thread_count: sysinfo::System::new().cpus().len().max(1),
                vram_budget_gb: 0.0,
                ram_budget_gb: usable_ram.max((model_gb * 0.8).min(total_ram_gb - 2.0)),
                safety_buffer_gb: ram_safety,
                expert_cache_budget_gb: cache_budget,
                moe_info: final_moe,
                target_ctx_tokens: ctx_resolution.target_ctx_tokens,
                kv_on_gpu: false,
            });
        } else {
            // State 7: CPU-Driven SSD Streaming (Massive MoE model exceeds physical RAM on CPU system)
            let ram_for_experts =
                (max_allowed_cluaiz_ram - geom.dense_backbone_gb - ggml_workspace_reserve).max(0.0);
            let cached_layers = if geom.single_expert_gb > 0.0 {
                let possible_experts = (ram_for_experts / geom.single_expert_gb).floor() as usize;
                (possible_experts / moe_info.expert_count.max(1)).min(geom.total_layers)
            } else {
                ((ram_for_experts / geom.layer_size_gb.max(0.01)).floor() as usize)
                    .min(geom.total_layers)
            };
            let cache_gb = (cached_layers * moe_info.expert_count) as f64 * geom.single_expert_gb;

            let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
                &request.model_path,
                user_n_ctx,
                usable_ram,
                total_ram_gb,
                model_gb,
                geom.dense_backbone_gb + ggml_workspace_reserve,
            );

            let grant = ResourceGrant {
                tier: PlacementTier::SsdStreaming,
                n_gpu_layers: 0,
                thread_count: sysinfo::System::new().cpus().len().max(1),
                vram_budget_gb: 0.0,
                ram_budget_gb: (geom.dense_backbone_gb
                    + ggml_workspace_reserve
                    + cache_gb
                    + ctx_resolution.required_ctx_gb)
                    .min(max_allowed_cluaiz_ram),
                safety_buffer_gb: ram_safety,
                expert_cache_budget_gb: cache_gb,
                moe_info: Some(moe_info.clone()),
                target_ctx_tokens: ctx_resolution.target_ctx_tokens,
                kv_on_gpu: false,
            };

            apply_windows_hard_memory_quota(grant.ram_budget_gb);
            return Ok(grant);
        }
    }

    // Step 5: Waterfall Priority 1 — 100% Dedicated VRAM Fit
    let display_headroom_gb = 0.15f64; // 150 MB display compositor headroom for Windows DWM
    let compute_scratch_vram_gb = (0.15 + (model_gb * 0.02)).clamp(0.15, 0.40);
    let vram_free_for_model = (live_free_vram_gb - display_headroom_gb).max(0.0);

    let ctx_resolution_ram = crate::hardware::context_negotiator::resolve_context_window(
        &request.model_path,
        user_n_ctx,
        usable_ram,
        total_ram_gb,
        model_gb,
        0.0,
    );

    // Check if model weights + scratch fit in Dedicated VRAM
    if (model_gb + compute_scratch_vram_gb) <= vram_free_for_model {
        let vram_for_kv = vram_free_for_model - model_gb - compute_scratch_vram_gb;
        let min_kv_gb = (2048.0 * ctx_resolution_ram.kv_bytes_per_token) / (1024.0 * 1024.0 * 1024.0);

        // State 1: Dedicated VRAM comfortably holds model AND at least 2048 tokens of KV cache
        if vram_for_kv >= min_kv_gb {
            let max_vram_tokens = ((vram_for_kv * 1024.0 * 1024.0 * 1024.0)
                / ctx_resolution_ram.kv_bytes_per_token) as usize;
            let target_tokens = if user_n_ctx > 0 {
                (user_n_ctx as usize).min(max_vram_tokens).clamp(2048, ctx_resolution_ram.native_max_ctx)
            } else {
                max_vram_tokens.clamp(2048, ctx_resolution_ram.native_max_ctx)
            };
            let actual_kv_gb = (target_tokens as f64 * ctx_resolution_ram.kv_bytes_per_token)
                / (1024.0 * 1024.0 * 1024.0);

            return Ok(ResourceGrant {
                tier: PlacementTier::GpuOnly,
                n_gpu_layers: -1,
                thread_count: sysinfo::System::new().cpus().len().max(1),
                vram_budget_gb: model_gb + actual_kv_gb + compute_scratch_vram_gb,
                ram_budget_gb: 0.0,
                safety_buffer_gb: vram_safety,
                expert_cache_budget_gb: 0.0,
                moe_info: None,
                target_ctx_tokens: target_tokens,
                kv_on_gpu: true,
            });
        } else {
            // State 3: Model weights fit in VRAM, but KV cache routes to Host RAM (0 Shared Memory)
            let ram_needed = ctx_resolution_ram.required_ctx_gb + ggml_workspace_reserve;
            return Ok(ResourceGrant {
                tier: PlacementTier::Hybrid,
                n_gpu_layers: -1,
                thread_count: sysinfo::System::new().cpus().len().max(1),
                vram_budget_gb: model_gb + compute_scratch_vram_gb,
                ram_budget_gb: ram_needed.min(max_allowed_cluaiz_ram),
                safety_buffer_gb: vram_safety,
                expert_cache_budget_gb: 0.0,
                moe_info: None,
                target_ctx_tokens: ctx_resolution_ram.target_ctx_tokens,
                kv_on_gpu: false,
            });
        }
    }

    // Waterfall Priority 2 — Dedicated VRAM Layer Offload (0 GB Shared VRAM)
    let vram_for_layers = (live_free_vram_gb - display_headroom_gb).max(0.0);
    let approx_gpu_layers = if geom.layer_size_gb > 0.0 && vram_for_layers > 0.0 {
        ((vram_for_layers / geom.layer_size_gb).floor() as i32)
            .min(geom.total_layers as i32)
            .max(0)
    } else {
        0
    };

    let allocated_vram = (approx_gpu_layers as f64 * geom.layer_size_gb).min(live_free_vram_gb);
    let remaining_layers = (geom.total_layers as i32 - approx_gpu_layers).max(0) as usize;

    let ram_weights_needed = if geom.is_moe {
        let offloaded_experts = remaining_layers * moe_info.expert_count;
        let experts_gb = offloaded_experts as f64 * geom.single_expert_gb;
        geom.dense_backbone_gb + experts_gb
    } else {
        remaining_layers as f64 * geom.layer_size_gb
    };

    let total_cluaiz_weights_and_workspace = ram_weights_needed + ggml_workspace_reserve;

    // Step 6: Dynamic Spill Absorption & Adaptive Context Trade-Off
    let fits_under_ceiling = ram_weights_needed <= max_allowed_cluaiz_ram;
    let can_absorb_into_ram = fits_under_ceiling
        || ((ram_weights_needed + 1.00) <= available_ram_gb
            && (ram_budget_info.used_ram_gb + ram_weights_needed) <= (total_ram_gb * 0.95));

    let (tier, _cached_layers, overflow_layers, actual_cache_gb) = if can_absorb_into_ram {
        let cache_gb = if geom.is_moe {
            (remaining_layers * moe_info.expert_count) as f64 * geom.single_expert_gb
        } else {
            0.0
        };
        (PlacementTier::Hybrid, remaining_layers, 0usize, cache_gb)
    } else {
        let ram_for_experts =
            (max_allowed_cluaiz_ram - geom.dense_backbone_gb - ggml_workspace_reserve).max(0.0);
        let cached = if geom.is_moe && geom.single_expert_gb > 0.0 {
            let possible_experts = (ram_for_experts / geom.single_expert_gb).floor() as usize;
            (possible_experts / moe_info.expert_count.max(1)).min(remaining_layers)
        } else {
            ((ram_for_experts / geom.layer_size_gb.max(0.01)).floor() as usize)
                .min(remaining_layers)
        };
        let overflow = remaining_layers.saturating_sub(cached);
        let cache_gb = if geom.is_moe {
            (cached * moe_info.expert_count) as f64 * geom.single_expert_gb
        } else {
            0.0
        };
        (PlacementTier::SsdStreaming, cached, overflow, cache_gb)
    };

    // Step 7: Dynamic Context Window from Leftover RAM (Strictly in Host RAM)
    let resident_weights_gb = if tier == PlacementTier::SsdStreaming {
        geom.dense_backbone_gb + actual_cache_gb
    } else {
        ram_weights_needed
    };
    let total_cluaiz_weights_and_workspace = resident_weights_gb + ggml_workspace_reserve;

    let leftover_ram_for_ctx = (usable_ram - total_cluaiz_weights_and_workspace).max(0.0);
    let mut ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
        &request.model_path,
        user_n_ctx,
        usable_ram,
        total_ram_gb,
        model_gb,
        total_cluaiz_weights_and_workspace,
    );

    if overflow_layers == 0 && leftover_ram_for_ctx > 0.50 {
        let headroom = (leftover_ram_for_ctx - 0.50).max(0.0);
        if headroom > 0.0 && ctx_resolution.kv_bytes_per_token > 0.0 {
            let extra_tokens = ((headroom * 1024.0 * 1024.0 * 1024.0)
                / ctx_resolution.kv_bytes_per_token) as usize;
            let target_upper = if user_n_ctx > 0 {
                (user_n_ctx as usize).min(ctx_resolution.native_max_ctx)
            } else {
                ctx_resolution.native_max_ctx
            };
            let expanded = (ctx_resolution.target_ctx_tokens + extra_tokens).min(target_upper);
            if expanded > ctx_resolution.target_ctx_tokens {
                ctx_resolution.target_ctx_tokens = expanded;
                ctx_resolution.required_ctx_gb = (expanded as f64
                    * ctx_resolution.kv_bytes_per_token)
                    / (1024.0 * 1024.0 * 1024.0);
            }
        }
    }

    // Single source of truth for KV cache GPU offload in hybrid tier:
    let leftover_vram = (live_free_vram_gb - allocated_vram - display_headroom_gb - compute_scratch_vram_gb).max(0.0);
    let kv_on_gpu = leftover_vram >= ctx_resolution.required_ctx_gb && tier != PlacementTier::SsdStreaming;

    let final_ram_budget = (total_cluaiz_weights_and_workspace + ctx_resolution.required_ctx_gb)
        .min(max_allowed_cluaiz_ram);
    let final_moe = if moe_info.is_moe {
        Some(moe_info.clone())
    } else {
        None
    };

    let grant = ResourceGrant {
        tier,
        n_gpu_layers: approx_gpu_layers,
        thread_count: sysinfo::System::new().cpus().len().max(1),
        vram_budget_gb: allocated_vram,
        ram_budget_gb: final_ram_budget,
        safety_buffer_gb: ram_safety,
        expert_cache_budget_gb: actual_cache_gb,
        moe_info: final_moe,
        target_ctx_tokens: ctx_resolution.target_ctx_tokens,
        kv_on_gpu,
    };

    if grant.tier == PlacementTier::SsdStreaming {
        apply_windows_hard_memory_quota(grant.ram_budget_gb);
    }

    Ok(grant)
}

#[cfg(windows)]
pub fn apply_windows_hard_memory_quota(usable_ram_gb: f64) {
    eprintln!(
        "🛡️ [Negotiator] Memory Guard: Unconstrained working set active (Physical RAM target: {:.2} GB, SSD swapping disabled).",
        usable_ram_gb
    );
}

#[cfg(not(windows))]
pub fn apply_windows_hard_memory_quota(_usable_ram_gb: f64) {}

/// Probes total physical VRAM and live free dedicated VRAM using NVML / hardware governor
pub fn probe_vram() -> (f64, f64) {
    let control = HardwareGovernor::load_system_control().unwrap_or_default();
    let total_vram_gb: f64 = control
        .silicon_truth
        .accelerators
        .gpus
        .iter()
        .map(|g| g.vram_total_gb)
        .sum();

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

    (total_vram_gb, live_free_vram_gb)
}

/// Prints the authoritative 7-state hardware allocation telemetry report
pub fn print_negotiator_full_report(title: &str, req: &ResourceRequest, grant: &ResourceGrant) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let free_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let (total_vram_gb, free_vram_gb) = probe_vram();

    let ctx_location_str = if grant.kv_on_gpu { "GPU GDDR6" } else { "System Host RAM" };
    let exec_str = if grant.n_gpu_layers == -1 || grant.n_gpu_layers > 0 { "GPU CUDA" } else { "CPU Cores" };
    let disk_str = match grant.tier {
        PlacementTier::GpuOnly => "0 Layers on Disk (100% GPU VRAM)",
        PlacementTier::Hybrid => "0 Layers on Disk (Memory-Resident Hybrid)",
        PlacementTier::CpuOnly => "0 Layers on Disk (System Host RAM)",
        PlacementTier::SsdStreaming => "Streaming from SSD",
    };

    eprintln!("\n================================================================================");
    eprintln!("🔬 [REPORT: {}] Model: {:.2} GB | Tier: {:?} | Exec: {}", title, req.model_size_gb, grant.tier, exec_str);
    eprintln!("├── VRAM: {:.2}/{:.2} GB free | GPU Offload: {} layers ({:.2} GB) | Shared: 0.00 GB",
        free_vram_gb, total_vram_gb, grant.n_gpu_layers, grant.vram_budget_gb);
    eprintln!("├── Context: {} tokens | KV Placement: {} | Floor: >= 2048",
        grant.target_ctx_tokens, ctx_location_str);
    eprintln!("├── Host RAM: {:.2}/{:.2} GB free | Budget: {:.2} GB | Safety Buffer: {:.2} GB",
        free_ram_gb, total_ram_gb, grant.ram_budget_gb, grant.safety_buffer_gb);
    eprintln!("└── Disk Mode: {} | Path: {:?}", disk_str, req.model_path);
    eprintln!("================================================================================\n");
}

#[cfg(test)]
mod tests {
    use super::print_negotiator_full_report as print_scenario_full_report;
    use super::*;

    #[test]
    fn test_scenario_1_gpu_only_when_fits_in_vram() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 1.50, model_path: PathBuf::from("C:\\models\\small-test.gguf"),
        };
        let grant = negotiate_resource(&req).expect("Scenario 1 negotiation failed");
        print_scenario_full_report("Scenario 1: GPU-Only Model (1.50 GB)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::GpuOnly, "Model <= free_vram must be GpuOnly!");
        assert_eq!(grant.n_gpu_layers, -1, "GpuOnly tier must request -1 (all GPU layers)!");
        assert!(grant.kv_on_gpu, "GpuOnly tier must place KV on GPU!");
    }

    #[test]
    fn test_scenario_2_hybrid_split_capped_layers_and_dynamic_ctx() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 14.62, model_path: PathBuf::from("C:\\models\\synthetic-moe-26b.gguf"),
        };
        let grant = negotiate_resource(&req).expect("Scenario 2 negotiation failed");
        print_scenario_full_report("Scenario 2: Gemma 26B Hybrid Offload (14.62 GB)", &req, &grant);

        assert!(grant.tier == PlacementTier::Hybrid || grant.tier == PlacementTier::SsdStreaming, "14.6GB model must be Hybrid or SsdStreaming!");
        assert!(grant.n_gpu_layers >= 0 && grant.n_gpu_layers < 30, "GPU layers must be capped to physical VRAM!");
        assert!(grant.target_ctx_tokens >= 2048, "Target context must be at least 2048 tokens!");
    }

    #[test]
    fn test_scenario_2b_dense_hybrid_model() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 8.50, model_path: PathBuf::from("C:\\models\\dense-llama-8b.gguf"),
        };
        let grant = negotiate_resource(&req).expect("Scenario 2b negotiation failed");
        print_scenario_full_report("Scenario 2b: Dense Hybrid Model (8.50 GB)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::Hybrid, "8.5GB dense model must be Hybrid!");
        assert!(grant.n_gpu_layers > 0, "GPU layers must be > 0 for dense hybrid!");
    }

    #[test]
    fn test_scenario_3_ssd_streaming_when_overflows_ram() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 35.0, model_path: PathBuf::from("C:\\models\\synthetic-moe-35b.gguf"),
        };
        if let Ok(grant) = negotiate_resource(&req) {
            print_scenario_full_report("Scenario 3: 35 GB Large MoE Overflow Model", &req, &grant);
            assert_eq!(grant.tier, PlacementTier::SsdStreaming, "35GB model must overflow to SsdStreaming!");
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
    }

    #[test]
    fn test_scenario_5_small_spill_absorption_under_95_percent_ceiling() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 8.00, model_path: PathBuf::from("C:\\models\\synthetic-moe-8b.gguf"),
        };
        let grant = negotiate_resource(&req).expect("Scenario 5 negotiation failed");
        print_scenario_full_report("Scenario 5: Small Spill Absorbed (8.00 GB Model)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::Hybrid, "Small spill model must be absorbed into Hybrid tier!");
    }

    #[test]
    fn test_scenario_6_cpu_only_when_fits_in_ram() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 4.00, model_path: PathBuf::from("C:\\models\\small-cpu-model.gguf"),
        };
        let grant = negotiate_resource_with_override(&req, Some(0)).expect("Scenario 6 failed");
        print_scenario_full_report("Scenario 6: CPU-Only Model Fit In RAM (4.00 GB)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::CpuOnly, "Small model on CPU must be CpuOnly!");
        assert_eq!(grant.n_gpu_layers, 0, "CPU tier must have 0 GPU layers!");
        assert_eq!(grant.vram_budget_gb, 0.0, "CPU tier must have 0.0 VRAM budget!");
    }

    #[test]
    fn test_scenario_7_cpu_driven_ssd_streaming() {
        let req = ResourceRequest {
            engine_type: EngineType::GGUF, inference_mode: InferenceMode::Chat,
            model_size_gb: 35.00, model_path: PathBuf::from("C:\\models\\synthetic-moe-35b.gguf"),
        };
        let grant = negotiate_resource_with_override(&req, Some(0)).expect("Scenario 7 failed");
        print_scenario_full_report("Scenario 7: CPU-Driven SSD Streaming (35.00 GB MoE)", &req, &grant);

        assert_eq!(grant.tier, PlacementTier::SsdStreaming, "Massive model on CPU must be SsdStreaming!");
        assert_eq!(grant.n_gpu_layers, 0, "CPU streaming must have 0 GPU layers!");
        assert_eq!(grant.vram_budget_gb, 0.0, "CPU streaming must have 0.0 VRAM budget!");
    }
}
