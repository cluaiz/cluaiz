use crate::define_config;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct DynamicConfigValue(pub String);

impl Default for DynamicConfigValue {
    fn default() -> Self {
        Self("{}".to_string())
    }
}

impl Serialize for DynamicConfigValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val: serde_json::Value = serde_json::from_str(&self.0).unwrap_or(serde_json::Value::Null);
        val.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DynamicConfigValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        Ok(DynamicConfigValue(val.to_string()))
    }
}

impl DynamicConfigValue {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::from_str(&self.0).unwrap_or(serde_json::Value::Null)
    }

    pub fn has_short_mode(&self) -> bool {
        self.0.to_lowercase().contains("short")
    }

    pub fn len(&self) -> usize {
        if let serde_json::Value::Object(map) = self.to_value() {
            map.len()
        } else {
            0
        }
    }

    pub fn insert(&mut self, key: String, value: String) {
        let mut val = self.to_value();
        if let serde_json::Value::Object(ref mut map) = val {
            map.insert(key, serde_json::Value::String(value));
            self.0 = val.to_string();
        }
    }
}

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct GgufHardwareExecution {
    pub n_gpu_layers: i32,
    pub no_mmap: bool,
    pub override_tensor: String,
    pub batch_size: usize,
    pub ubatch_size: usize,
    pub cache_type_k: String,
    pub cache_type_v: String,
    pub parallel: usize,
    pub spec_type: String,
    pub spec_draft_n_max: usize,
}

impl Default for GgufHardwareExecution {
    fn default() -> Self {
        Self {
            n_gpu_layers: 0,
            no_mmap: false,
            override_tensor: String::new(),
            batch_size: 512,
            ubatch_size: 512,
            cache_type_k: "f16".to_string(),
            cache_type_v: "f16".to_string(),
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
    pub repeat_penalty: f64,
}

impl Default for GgufSamplers {
    fn default() -> Self {
        Self {
            temp: 0.8,
            top_p: 0.95,
            top_k: 40,
            min_p: 0.05,
            presence_penalty: 0.0,
            repeat_penalty: 1.1,
        }
    }
}

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct UserMovedFlags {
    pub think_mode: String,
    pub response_length: DynamicConfigValue,
}

impl Default for UserMovedFlags {
    fn default() -> Self {
        Self {
            think_mode: "Auto".to_string(),
            response_length: DynamicConfigValue::default(),
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
