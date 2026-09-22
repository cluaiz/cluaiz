use crate::define_config;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct GgufHardwareExecution {
    pub n_gpu_layers: i32,
    pub n_ctx: i32,
    pub no_mmap: bool,
    pub override_tensor: String,
    pub batch_size: usize,
    pub ubatch_size: usize,

    pub parallel: usize,
    pub spec_type: String,
    pub spec_draft_n_max: usize,
}

impl Default for GgufHardwareExecution {
    fn default() -> Self {
        Self {
            n_gpu_layers: 0,
            n_ctx: 0,
            no_mmap: false,
            override_tensor: String::new(),
            batch_size: 512,
            ubatch_size: 512,
            parallel: 1,
            spec_type: String::new(),
            spec_draft_n_max: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct GgufTemplatingFlags {
    pub chat_template_file: String,
    pub chat_template_kwargs: String,
    pub jinja: bool,
    pub fit: String,
}

impl Default for GgufTemplatingFlags {
    fn default() -> Self {
        Self {
            chat_template_file: String::new(),
            chat_template_kwargs: String::new(),
            jinja: false,
            fit: "off".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct GgufSamplers {
    pub temp: f64,
    pub top_p: f64,
    pub top_k: usize,
    pub min_p: f64,
    pub presence_penalty: f64,
    #[serde(default)]
    pub frequency_penalty: f64,
    pub repeat_penalty: f64,
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Default for GgufSamplers {
    fn default() -> Self {
        Self {
            temp: 0.8,
            top_p: 0.95,
            top_k: 40,
            min_p: 0.05,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
            repeat_penalty: 1.1,
            seed: None,
        }
    }
}

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct UserMovedFlags {
    pub think_mode: String,
    pub response_length: String,
}

impl Default for UserMovedFlags {
    fn default() -> Self {
        Self {
            think_mode: "Auto".to_string(),
            response_length: "auto".to_string(),
        }
    }
}

#[derive(
    Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug, Default,
)]
pub struct GgufMetadataHeaders {
    pub hardware_and_execution: GgufHardwareExecution,
    pub templating_flags: GgufTemplatingFlags,
    pub samplers: GgufSamplers,
    pub user_moved_flags: UserMovedFlags,
}

// Generate the fast zero-copy load/save functions using the macro!
define_config!(GgufMetadataHeaders, "gguf_metadata_headers");
