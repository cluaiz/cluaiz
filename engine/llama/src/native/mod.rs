//! 🚀 Native Safe Llama Engine Wrapper
//! Direct bindings into compiled llama.cpp C/C++ memory.

pub mod core;
pub mod stream;
pub mod sampler;
pub mod context;
pub mod templater;

pub use core::{route_llama_logs, GenerationTimings, NativeLlama, ThinkingReport};
pub use stream::stream_tokens;
pub use templater::extract_thinking_tags_native;
