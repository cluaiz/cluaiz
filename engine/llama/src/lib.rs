//! 🦙 Cluaiz Llama Engine (`engine/llama`)
//! Safe Rust API wrapping compiled llama.cpp C/C++ memory directly as an `rlib`.
//! Statically linked into the unified process space, eliminating dual-memory static copy bugs.

pub mod ffi;
pub mod native;
pub mod config;
pub mod sampling;
pub mod moe_override;

pub use native::{GenerationTimings, NativeLlama, ThinkingReport, stream_tokens, extract_thinking_tags_native};
pub use config::OptimizationConfig;
