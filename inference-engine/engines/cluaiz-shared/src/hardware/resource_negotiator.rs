//! ⚖️ Unified Resource Negotiator
//! Single Source of Truth for hardware placement decisions across ALL engines (GGUF/ONNX)
//! and ALL inference modes (Chat/Embedding/Audio/TTS).
//!
//! Tiered Waterfall: GPU VRAM → Shared VRAM+RAM (Hybrid) → CPU RAM → SSD Streaming

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::hardware::schema::optimization::{OptimizationControl, FeatureState};
use crate::hardware::governor::HardwareGovernor;

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
    /// Negotiated safe context window (tokens)
    pub max_context: usize,
    /// CPU threads allocated for this engine
    pub thread_count: usize,
    /// VRAM budget in GB this engine may use
    pub vram_budget_gb: f64,
    /// System RAM budget in GB this engine may use
    pub ram_budget_gb: f64,
    /// OS safety buffer reserved (GB)
    pub safety_buffer_gb: f64,
}

// ─── Safety Margin Calculation ───────────────────────────────────────────────

/// Calculates the OS safety buffer in GB based on user settings.
/// Supports two modes:
///   1. Direct GB mode: `custom_vram_buffer_gb` is set (e.g., 1.5 GB fixed)
///   2. Percentage mode: Calculated from `BoosterMode` profile
pub fn calculate_safety_buffer(booster: &OptimizationControl, total_vram_gb: f64) -> f64 {
    // Priority 1: User specified a direct GB VRAM buffer (e.g. 1.5 GB)
    if let Some(direct_gb) = booster.custom_vram_buffer_gb {
        if direct_gb > 0.0 {
            return direct_gb;
        }
    }

    // Priority 2: Auto Mode — dynamic capacity-scaled safety margin
    let floor_gb = 0.60; // 600MB OS safety floor
    let margin_pct = 0.15f64.min(2.0 / total_vram_gb.max(0.1));
    let mut buffer_gb = total_vram_gb * margin_pct;

    if total_vram_gb < 24.0 {
        buffer_gb = buffer_gb.max(floor_gb);
    }

    if booster.force_vram_reclaim == FeatureState::On {
        buffer_gb = (total_vram_gb * 0.005).max(0.25);
    }

    buffer_gb
}

/// Calculates the OS safety buffer for CPU RAM in GB based on user settings.
pub fn calculate_ram_safety_buffer(booster: &OptimizationControl, total_ram_gb: f64) -> f64 {
    if let Some(direct_gb) = booster.custom_ram_buffer_gb {
        if direct_gb > 0.0 {
            return direct_gb;
        }
    }

    // Auto Mode for RAM: Default 10% safety margin, minimum 1.0 GB floor
    let floor_gb = 1.0;
    let buffer_gb = (total_ram_gb * 0.10).max(floor_gb);
    buffer_gb
}

// ─── Core Negotiation Logic ──────────────────────────────────────────────────

/// The unified resource negotiation function.
/// Called by ALL engines (GGUF/ONNX) and ALL modes (Chat/Embedding/Audio/TTS).
///
/// Reads:
///   - `system_control.json` → total VRAM, total RAM
///   - `llm_optimization.json` → BoosterMode, custom buffer
///   - Engine-specific metadata (gguf/onnx headers) → user `n_gpu_layers` setting
///
/// Returns a `ResourceGrant` with tier, GPU layers, and memory budgets.
pub fn negotiate_resource(request: &ResourceRequest) -> anyhow::Result<ResourceGrant> {
    // ─── Step 1: Read Silicon Truth ───
    let control = HardwareGovernor::load_system_control()
        .unwrap_or_default();

    let total_vram_gb: f64 = control
        .silicon_truth
        .accelerators
        .gpus
        .iter()
        .map(|g| g.vram_total_gb)
        .sum();

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram_gb = (sys.total_memory() as f64) / (1024.0 * 1024.0 * 1024.0);
    let available_ram_gb = (sys.available_memory() as f64) / (1024.0 * 1024.0 * 1024.0);

    // ─── Step 2: Read User Settings ───
    let booster = HardwareGovernor::load_booster_settings().unwrap_or_default();

    // ─── Step 3: Read Engine-Specific n_gpu_layers ───
    let user_n_gpu_layers = match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_gpu_layers
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load()
                .n_gpu_layers
        }
    };

    // ─── Step 4: Calculate Safety Buffer ───
    let vram_safety = calculate_safety_buffer(&booster, total_vram_gb);
    let ram_safety = 1.5f64; // Always reserve 1.5GB for OS in RAM

    let usable_vram = (total_vram_gb - vram_safety).max(0.0);
    let usable_ram = (available_ram_gb - ram_safety).max(0.0);

    // ─── Step 5: Check Existing ARBITER Allocations ───
    let existing_allocs: f64 = HardwareGovernor::get_active_allocations()
        .iter()
        .map(|p| p.vram_gb)
        .sum();
    let free_vram = (usable_vram - existing_allocs).max(0.0);

    let model_gb = request.model_size_gb;

    // ─── Step 6: User Override Check ───
    // If user explicitly set n_gpu_layers = 0, force CPU-only regardless of VRAM
    if user_n_gpu_layers == 0 {
        tracing::info!(
            "⚖️ [Negotiator] CPU-Only mode forced by user (n_gpu_layers = 0). \
             Model: {:.2}GB, Free RAM: {:.2}GB",
            model_gb, usable_ram
        );
        return Ok(ResourceGrant {
            tier: PlacementTier::CpuOnly,
            n_gpu_layers: 0,
            max_context: 4096, // Conservative default; governor refines this later
            thread_count: sysinfo::System::new().cpus().len().max(1),
            vram_budget_gb: 0.0,
            ram_budget_gb: usable_ram.min(model_gb * 1.2), // Model + 20% headroom
            safety_buffer_gb: ram_safety,
        });
    }

    // ─── Step 7: Tiered Placement Decision ───
    let (tier, n_gpu_layers, vram_budget, ram_budget) = if total_vram_gb < 0.1 {
        // No GPU detected at all
        tracing::info!(
            "⚖️ [Negotiator] No GPU detected → CPU Only. Model: {:.2}GB",
            model_gb
        );
        (PlacementTier::CpuOnly, 0i32, 0.0, usable_ram.min(model_gb * 1.2))

    } else if model_gb <= free_vram {
        // Tier 1: Model fits entirely in free VRAM
        tracing::info!(
            "⚖️ [Negotiator] ✅ Tier 1 (GPU Only). Model: {:.2}GB ≤ Free VRAM: {:.2}GB",
            model_gb, free_vram
        );
        (PlacementTier::GpuOnly, -1, model_gb, 0.0)

    } else if model_gb <= (free_vram + usable_ram) {
        // Tier 2: Split across VRAM + RAM
        let ram_share = model_gb - free_vram;
        tracing::info!(
            "⚖️ [Negotiator] ⚡ Tier 2 (Hybrid). Model: {:.2}GB → VRAM: {:.2}GB + RAM: {:.2}GB",
            model_gb, free_vram, ram_share
        );
        // Calculate approximate GPU layer count for partial offload
        let gpu_ratio = free_vram / model_gb.max(0.01);
        let approx_layers = (gpu_ratio * 40.0) as i32; // Rough estimate assuming ~40 layers
        (PlacementTier::Hybrid, approx_layers.max(1), free_vram, ram_share)

    } else if model_gb <= usable_ram {
        // Tier 3: CPU only (model fits in RAM but not VRAM)
        tracing::info!(
            "⚖️ [Negotiator] 💻 Tier 3 (CPU Only). Model: {:.2}GB ≤ Free RAM: {:.2}GB",
            model_gb, usable_ram
        );
        (PlacementTier::CpuOnly, 0, 0.0, model_gb)

    } else {
        // Tier 4: SSD Streaming required (model exceeds all available memory)
        tracing::warn!(
            "⚖️ [Negotiator] 💾 Tier 4 (SSD Streaming). Model: {:.2}GB exceeds VRAM({:.2}) + RAM({:.2})",
            model_gb, free_vram, usable_ram
        );
        // Placeholder: return error for now. MoE streaming will be implemented later.
        (PlacementTier::SsdStreaming, 0, 0.0, usable_ram)
    };

    let grant = ResourceGrant {
        tier,
        n_gpu_layers,
        max_context: 4096, // Conservative default; caller (governor) can refine with DNA
        thread_count: sysinfo::System::new().cpus().len().max(1),
        vram_budget_gb: vram_budget,
        ram_budget_gb: ram_budget,
        safety_buffer_gb: vram_safety,
    };

    tracing::info!(
        "⚖️ [Negotiator] Grant → {} | GPU Layers: {} | VRAM: {:.2}GB | RAM: {:.2}GB | Buffer: {:.2}GB",
        grant.tier, grant.n_gpu_layers, grant.vram_budget_gb, grant.ram_budget_gb, grant.safety_buffer_gb
    );

    Ok(grant)
}
