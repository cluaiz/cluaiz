pub mod family_adapter;
pub mod capabilities;
pub mod flow_matching;
pub mod neural_vocoder;
pub mod phoneme_map;
pub mod phoneme_sets;
pub mod languages;
pub mod rules_g2p;
mod numbers;
mod rbnf;
mod reading;
pub mod tts_router;
pub mod vocoder;
pub mod g2p;
pub mod manifest_loader;
pub mod families;
pub mod ipa_dictionary;

pub use family_adapter::{FamilyAdapter, TtsFamily};

/// Whether a registry `tts_family` tag is listed as experimental in `data/tts_families.json`
/// (never chosen automatically).
pub fn is_experimental_family(tag: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Families {
        experimental: Vec<String>,
    }
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES
        .get_or_init(|| {
            serde_json::from_str::<Families>(include_str!("data/tts_families.json"))
                .map(|f| f.experimental)
                .unwrap_or_default()
        })
        .iter()
        .any(|t| t.eq_ignore_ascii_case(tag))
}
pub use manifest_loader::TtsModelManifest;
pub use tts_router::route_tts_inference;
pub use vocoder::NativeVocoder;

