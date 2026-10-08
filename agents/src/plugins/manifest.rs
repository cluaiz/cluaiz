use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub discovery: Option<PluginDiscovery>,
    #[serde(default)]
    pub activation: Option<PluginActivation>,
    #[serde(default)]
    pub permissions: Option<PluginPermissions>,
    #[serde(default)]
    pub execution: Option<PluginExecution>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginDiscovery {
    #[serde(default)]
    pub semantic_triggers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginActivation {
    #[serde(default)]
    pub lazy_load: bool,
    #[serde(default)]
    pub trigger_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginPermissions {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_time_ms: Option<u64>,
    #[serde(default)]
    pub network_access: bool,
    #[serde(default)]
    pub vram_kv_inject: bool,
    #[serde(default)]
    pub file_system: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginExecution {
    #[serde(default = "default_envelope")]
    pub envelope: String, // "WASM" | "NATIVE"
    #[serde(default = "default_entry_point")]
    pub entry_point: String,
    #[serde(default = "default_payload_format")]
    pub payload_format: String,
    pub binary_path: Option<String>,
}

fn default_envelope() -> String {
    "WASM".to_string()
}

fn default_entry_point() -> String {
    "cluaiz_entry".to_string()
}

fn default_payload_format() -> String {
    "MsgPack".to_string()
}

pub struct PluginManifestParser;

impl PluginManifestParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Option<PluginManifest> {
        let content = std::fs::read_to_string(path).ok()?;
        
        let val = serde_json::from_str::<serde_json::Value>(&content).ok()?;
        let mut manifest = PluginManifest::default();
        if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
            manifest.name = id.to_string();
        } else if let Some(n) = val.get("name").and_then(|v| v.as_str()) {
            manifest.name = n.to_string();
        }
        if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
            manifest.description = desc.to_string();
        }
        let btype = val.get("build_type").and_then(|v| v.as_str()).unwrap_or("wasm");
        let mut exec = PluginExecution::default();
        exec.envelope = if btype == "binary" { "NATIVE".to_string() } else { "WASM".to_string() };
        manifest.execution = Some(exec);
        Some(manifest)
    }
}
