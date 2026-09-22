//! Sovereign Neural Core: The shared spine for all inference engines.
//! Decouples core logic (parameter resolution, ops, sampling) from backend kernels.

pub mod config;
pub mod ops;
pub mod sampling;

/// Common result type for neural operations
pub type NeuralResult<T> = Result<T, String>;
