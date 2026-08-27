use serde::{Deserialize, Serialize};
use std::path::Path;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpManifest {
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
    pub discovery: Option<McpDiscovery>,
    #[serde(default)]
    pub activation: Option<McpActivation>,
    #[serde(default)]
    pub permissions: Option<McpPermissions>,
    #[serde(default)]
    pub execution: Option<McpExecution>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpDiscovery {
    #[serde(default)]
    pub semantic_triggers: Vec<String>,
    pub cel_grammar: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpActivation {
    #[serde(default)]
    pub lazy_load: bool,
    #[serde(default)]
    pub trigger_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpPermissions {
    #[serde(default)]
    pub network_access: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub file_system: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpExecution {
    #[serde(default = "default_command")]
    pub command: String, // "npx", "node", "python"
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_command() -> String {
    "node".to_string()
}

pub struct McpManifestParser;

impl McpManifestParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Option<McpManifest> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_yaml::from_str(&content).ok()
    }
}
