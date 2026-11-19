//! 🏛️ Cluaiz Engine Runtime & Model Host (`engine/runtime`)
//! Central model host governing active execution plans and streaming sessions.

pub mod chat_memory;
pub mod embedding_host;
pub mod host;
pub mod load_events;
pub mod load_history;
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

/// The GPU backends llama.cpp loaded ("CUDA0: NVIDIA ..."); empty when models run on the CPU.
pub fn loaded_gpu_backends() -> Vec<String> {
    llama_engine::loaded_gpu_backends()
}
pub use llama_engine::{ContextReport, GenerationTimings, ThinkingReport};
/// Tells the model catalogue which quantizations the bundled llama.cpp can read, so a pull offers
/// only GGUF files that load (a llama.cpp fork's `PTQ1_0` or `PQ2_0` is left out). Call once at start.
pub fn register_loadable_quantizations() {
    cluaiz_models::taxonomy::set_loadable_tensor_types(llama_engine::tensor_type_names());
}

