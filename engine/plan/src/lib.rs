//! 🎯 Cluaiz Execution Planner (`engine/plan`)
//! Pure mathematical placement decisions (Candidates A-F, budgets, and context window).

pub mod types;
pub mod planner;

pub use types::{ExecutionPlan, ExecutionCandidate, PlacementTier};
pub use planner::{
    ram_budget_bytes, resolve_context, vram_buffer_options, ContextChoice, DeviceBudget, Planner,
    AUTO_CONTEXT_FLOOR, RAM_CEILING_FRACTION,
};
