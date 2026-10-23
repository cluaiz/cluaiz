//! Context planning for the Engine, kept pure: it gets token counts and returns decisions, and
//! never reads files, calls the model or touches the KV cache. The API and runtime crates do
//! the I/O around it.

pub mod compaction;
pub mod planner;
pub mod policy;

pub use compaction::{compact, CompactionInput, CompactionPlan, StepCost, TurnCost};
pub use planner::{plan, Class, Decision, Plan, PlanError, PlanInput, Segment};
pub use policy::Policy;
