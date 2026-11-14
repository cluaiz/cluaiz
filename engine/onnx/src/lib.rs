//! 🎙️ Cluaiz ONNX Audio & Multimodal Engine (`engine/onnx`)
//! Provides speech-to-text (Whisper), text-to-speech (Kokoro, CosyVoice, etc.), and vision encoders.

pub mod audio;
pub mod cudnn_driver;
pub mod text;
pub mod vision;
pub mod chat;
pub mod engine;
pub mod media;
pub mod ort_env;

pub use engine::OnnxEngine;
