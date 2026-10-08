//! 🎯 Cluaiz Execution Planner (`engine/plan`)
//! Pure mathematical placement decisions (Candidates A-F, budgets, and context window).

pub mod types;
pub mod planner;

pub use types::{ExecutionPlan, ExecutionCandidate, PlacementTier};
pub use planner::Planner;
