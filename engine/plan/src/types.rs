//! 📋 Typed Execution Plan Definitions
//! Represents pure decisions for model placement and context windows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementTier {
    GpuOnly,
    Hybrid,
    CpuOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCandidate {
    pub name: String,
    pub tier: PlacementTier,
    pub n_gpu_layers: i32,
    pub context_window: u32,
    pub vram_required_gb: f64,
    pub ram_required_gb: f64,
    pub estimated_tps: f64,
    pub feasible: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub model_id: String,
    pub selected_tier: PlacementTier,
    pub n_gpu_layers: i32,
    pub context_window: u32,
    pub total_layers: u32,
    pub vram_budget_gb: f64,
    pub ram_budget_gb: f64,
    pub candidates: Vec<ExecutionCandidate>,
    pub explanation: String,
}
