//! 🏛️ Cluaiz Engine Runtime & Model Host (`engine/runtime`)
//! Central model host governing active execution plans and streaming sessions.

pub mod chat_memory;
pub mod embedding_host;
pub mod host;
pub mod media_admission;
mod media_room;
pub mod media_support;
pub mod onnx_host;
pub mod speech_host;
mod speech_plan;
pub mod vram_use;

pub use host::{stream_error, EngineHost, HostResponse, ModelDescription, PlacementRefused};

/// The hardware a GGUF model is planned on: live free memory, and the GPUs llama.cpp's loaded
/// backends can use (Metal and Vulkan GPUs too, which NVML does not see).
pub fn gguf_hardware() -> cluaiz_hw::SystemInfo {
    host::native_load::gguf_hardware()
}
pub use llama_engine::{ContextReport, GenerationTimings, ThinkingReport};
