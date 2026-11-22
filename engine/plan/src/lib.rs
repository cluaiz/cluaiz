//! 🎯 Cluaiz Execution Planner (`engine/plan`)
//! Pure mathematical placement decisions (Candidates A-F, budgets, and context window).

pub mod types;
pub mod planner;
pub mod media_budget;
pub mod margin;
pub mod speed;
pub mod moe_cache;
pub mod expert_prefetch;
pub mod report;

pub use types::{ExecutionPlan, ExecutionCandidate, PlacementTier};
pub use planner::{
    ram_budget_bytes, resolve_context, vram_buffer_options, ContextChoice, DeviceBudget, Planner, StepDown,
    AUTO_CONTEXT_FLOOR, AUTO_CONTEXT_MIN, RAM_CEILING_FRACTION,
};
