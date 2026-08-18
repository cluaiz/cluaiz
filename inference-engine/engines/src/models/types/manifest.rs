use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAsset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationModel {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub architecture: String,
    #[serde(default)]
    pub parameters: String,
    #[serde(default)]
    pub training_tokens: String,
    #[serde(default = "default_bit_depth", deserialize_with = "deserialize_bit_depth")]
    pub bit_depth: f64,
    pub ram_required_gb: f64,
    #[serde(default)]
    pub download_size_gb: f64,
    pub huggingface_repo: String,
    pub download_url: String,
    #[serde(default)]
    pub description: String,
    pub is_cloud_api: bool,
    #[serde(default)]
    pub requires_gpu: bool,
    #[serde(default = "default_context")]
    pub context_window: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub assets: Vec<ModelAsset>,
    #[serde(default)]
    pub has_vision: bool,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub expert_count: Option<usize>,
    #[serde(default)]
    pub experts_per_token: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct InstallationFile {
    pub models: Vec<InstallationModel>,
}

pub fn default_bit_depth() -> f64 {
    4.0
}

pub fn deserialize_bit_depth<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BitDepthVisitor;

    impl<'de> serde::de::Visitor<'de> for BitDepthVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a float or a string containing a float")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            value.parse::<f64>().map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_any(BitDepthVisitor)
}

pub fn default_context() -> String {
    "8k".to_string()
}

pub fn default_category() -> String {
    "chat".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub architecture: String,
    #[serde(default)]
    pub architecture_type: String,
    pub parameters: String,
    pub training_tokens: String,
    #[serde(default = "default_bit_depth", deserialize_with = "deserialize_bit_depth")]
    pub bit_depth: f64,
    pub ram_required_gb: f64,
    pub download_size_gb: f64,
    pub huggingface_repo: String,
    pub huggingface_filename: String,
    pub download_url: String,
    pub description: String,
    pub is_cloud_api: bool,
    pub requires_gpu: bool,
    #[serde(default)]
    pub is_free_tier: bool,
    pub input_modality: String,
    pub context_window: String,
    pub family: String,
    pub category: String,
    pub assets: Vec<ModelAsset>,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub dna_path: Option<String>,
    #[serde(default)]
    pub has_vision: bool,
    #[serde(default)]
    pub has_audio: bool,
    pub expert_count: Option<usize>,
    pub experts_per_token: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RosterFile {
    pub models: Vec<ModelManifest>,
}

#[derive(Debug, Serialize)]
pub struct ModelRecommendation {
    pub manifest: ModelManifest,
    pub status: String,
    pub is_cached: bool,
}

// ─── Live Installed Registry Structs (model_registry.json) ────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryModelFile {
    pub name: String,
    pub size_bytes: u64,
    pub is_primary: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryModelMetadata {
    pub architecture: String,
    pub parameters: String,
    pub context_window: String,
    pub quantization: Option<String>,
    pub bit_depth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_start_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_end_tag: Option<String>,
    pub chat_template: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelRegistryEntry {
    pub id: String,
    pub category: String,
    pub format_type: String,
    pub huggingface_repo: String,
    pub local_dir: String,
    pub files: Vec<RegistryModelFile>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra_files: serde_json::Value,
    pub supported_tasks: Vec<String>,
    pub requires_gpu: bool,
    pub metadata: RegistryModelMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelRegistry {
    pub installed_models: HashMap<String, ModelRegistryEntry>,
}
