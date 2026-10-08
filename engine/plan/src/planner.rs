//! 🎯 Pure Placement Planner
//! Pure mathematical function taking (geometry, hardware, settings) -> ExecutionPlan.
//! Zero side-effects, 100% unit-testable.

use cluaiz_core::{bytes_to_gib, ActiveUserSettings};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use crate::types::{ExecutionCandidate, ExecutionPlan, PlacementTier};

pub struct Planner;

impl Planner {
    pub fn create_plan(
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let total_layers = geometry.block_count;
        let model_size_gb = bytes_to_gib(geometry.file_size_bytes);

        // Hardware envelopes
        let vram_free_gb = hardware.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
        let ram_free_gb = hardware.memory.available_gb;

        // Safety buffers
        let vram_buffer_gb = settings.optimization.custom_vram_buffer_gb.unwrap_or(0.5);
        let ram_buffer_gb = settings.optimization.custom_ram_buffer_gb.unwrap_or(1.0);

        let safe_vram_gb = (vram_free_gb - vram_buffer_gb).max(0.0);
        let safe_ram_gb = (ram_free_gb - ram_buffer_gb).max(0.0);

        // Target context
        let target_ctx = geometry.context_length.max(2048).min(8192);
        let kv_bytes = geometry.calculate_kv_cache_bytes(target_ctx, true);
        let kv_gb = bytes_to_gib(kv_bytes);

        let mut candidates = Vec::new();

        // Candidate A: GPU Only
        let gpu_required_gb = model_size_gb + kv_gb;
        let gpu_feasible = safe_vram_gb >= gpu_required_gb;
        candidates.push(ExecutionCandidate {
            name: "Candidate A (GPU Only)".into(),
            tier: PlacementTier::GpuOnly,
            n_gpu_layers: -1, // all layers
            context_window: target_ctx,
            vram_required_gb: gpu_required_gb,
            ram_required_gb: 0.2, // minimal host runtime overhead
            estimated_tps: 45.0,
            feasible: gpu_feasible,
            reason: if gpu_feasible { "Fits entirely within free VRAM.".into() } else { "Insufficient VRAM for full offload.".into() },
        });

        // Candidate B: Hybrid Split
        let layer_weight_gb = if total_layers > 0 { model_size_gb / (total_layers as f64) } else { 0.1 };
        let vram_for_layers = (safe_vram_gb - kv_gb).max(0.0);
        let offloadable_layers = if layer_weight_gb > 0.0 {
            ((vram_for_layers / layer_weight_gb).floor() as i32).min(total_layers as i32)
        } else {
            0
        };
        let hybrid_vram_gb = (offloadable_layers as f64 * layer_weight_gb) + kv_gb;
        let hybrid_ram_gb = ((total_layers as i32 - offloadable_layers).max(0) as f64 * layer_weight_gb) + 0.5;
        let hybrid_feasible = offloadable_layers > 0 && safe_vram_gb >= hybrid_vram_gb && safe_ram_gb >= hybrid_ram_gb;

        candidates.push(ExecutionCandidate {
            name: "Candidate B (Hybrid Split)".into(),
            tier: PlacementTier::Hybrid,
            n_gpu_layers: offloadable_layers,
            context_window: target_ctx,
            vram_required_gb: hybrid_vram_gb,
            ram_required_gb: hybrid_ram_gb,
            estimated_tps: 15.0 + (offloadable_layers as f64 * 0.5),
            feasible: hybrid_feasible,
            reason: if hybrid_feasible { format!("Splits {} layers to GPU, remainder to RAM.", offloadable_layers) } else { "Insufficient memory for hybrid split.".into() },
        });

        // Candidate C: CPU Only
        let cpu_required_gb = model_size_gb + kv_gb;
        let cpu_feasible = safe_ram_gb >= cpu_required_gb;
        candidates.push(ExecutionCandidate {
            name: "Candidate C (CPU Only)".into(),
            tier: PlacementTier::CpuOnly,
            n_gpu_layers: 0,
            context_window: target_ctx.min(4096),
            vram_required_gb: 0.0,
            ram_required_gb: cpu_required_gb,
            estimated_tps: 6.0,
            feasible: cpu_feasible,
            reason: if cpu_feasible { "Model fits completely in host system RAM.".into() } else { "System RAM insufficient.".into() },
        });

        // Choose best feasible candidate
        let selected = candidates
            .iter()
            .find(|c| c.feasible)
            .cloned()
            .unwrap_or_else(|| candidates[2].clone());

        ExecutionPlan {
            model_id: model_id.to_string(),
            selected_tier: selected.tier,
            n_gpu_layers: selected.n_gpu_layers,
            context_window: selected.context_window,
            total_layers,
            vram_budget_gb: selected.vram_required_gb,
            ram_budget_gb: selected.ram_required_gb,
            candidates,
            explanation: selected.reason,
        }
    }
}
