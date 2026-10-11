//! 🎙️ Cluaiz ONNX Audio & Multimodal Engine (`engine/onnx`)
//! Provides speech-to-text (Whisper), text-to-speech (Kokoro, CosyVoice, etc.), and vision encoders.

pub mod audio;
pub mod text;
pub mod vision;
pub mod chat;
pub mod metadata;
pub mod engine;

pub use engine::OnnxEngine;
pub use metadata::OnnxMetadataHeaders;
