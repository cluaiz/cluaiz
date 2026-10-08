pub mod tags;
pub mod heuristics;
pub mod tts_families;
pub mod stt_families;
pub mod rules;
pub mod classifier;
pub mod quantization;

pub use tags::*;
pub use heuristics::*;
pub use tts_families::{TtsFamily, TtsTaxonomy};
pub use stt_families::{SttFamily, SttTaxonomy};
pub use rules::{ModelCapabilities, UniversalTaskRules};
pub use classifier::{ClassificationResult, UniversalModelClassifier};
pub use quantization::UniversalQuantization;
