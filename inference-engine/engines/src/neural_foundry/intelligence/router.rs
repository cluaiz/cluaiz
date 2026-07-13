//! # Sovereign Intelligence Router
//! 
//! This module acts as the central hub for routing requests to the appropriate embedding engine.
//! Depending on the user's hardware and settings, it dispatches vector generation tasks to:
//! - **ONNX (Default):** For low-latency CPU-based multimodal embeddings (Text, Audio, Image).
//! - **LLaMA:** For heavy GPU-based embeddings (e.g., GGUF).
//! - **External APIs:** For cloud-based fallbacks.
//!
//! The actual core trait `EmbeddingDriver` is defined in `neural_core::interfaces::router_contract`
//! to ensure all external engines (like the standalone `onnx` crate) can depend on it without
//! creating circular dependencies with the main engine codebase.

pub use neural_core::interfaces::router_contract::{
    EmbeddingDriver, EngineError, Modality,
};
