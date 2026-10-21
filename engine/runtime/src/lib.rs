//! 🏛️ Cluaiz Engine Runtime & Model Host (`engine/runtime`)
//! Central model host governing active execution plans and streaming sessions.

pub mod chat_memory;
pub mod embedding_host;
pub mod host;
pub mod media_admission;
pub mod media_support;
pub mod onnx_host;
pub mod speech_host;
mod speech_plan;

pub use host::{stream_error, EngineHost, HostResponse, ModelDescription};
pub use llama_engine::{ContextReport, GenerationTimings, ThinkingReport};
