//! 🏛️ Cluaiz Engine Runtime & Model Host (`engine/runtime`)
//! Central model host governing active execution plans and streaming sessions.

pub mod host;
pub mod media_support;
pub mod onnx_host;

pub use host::{stream_error, EngineHost, HostResponse, ModelDescription};
pub use llama_engine::{GenerationTimings, ThinkingReport};
