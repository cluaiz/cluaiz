//! 🏛️ Sovereign Booster Manager: Orchestration Core
//! Professional management of neural boosters and conflict resolution.

pub mod conflict_resolver;
pub mod dependency_graph;
pub mod priority_scheduler;
pub mod auto_tuner;

pub use conflict_resolver::ConflictResolver;
pub use auto_tuner::AutoTuner;
