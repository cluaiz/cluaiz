//! 🚀 Native Safe Llama Engine Wrapper
//! Direct bindings into compiled llama.cpp C/C++ memory.

pub mod core;
pub mod stream;
pub mod sampler;
pub mod context;
pub mod templater;
pub mod media;
pub mod session;
pub mod speech;
pub mod embedding;
pub mod load_steps;
pub mod projection;
pub mod speculative;
mod tokenizer;
pub use tokenizer::VocabHandle;

pub use core::{gpu_offload_available, loaded_gpu_backends, route_llama_logs, ContextExceeded, ContextReport, GenerationTimings, MtpReport, NativeLlama, ThinkingReport};
pub use stream::stream_tokens;
pub use templater::extract_thinking_tags_native;
