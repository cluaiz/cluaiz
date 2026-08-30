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
    pub command: String, // "npx", "node", "python", "uvx"
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_command() -> String {
    "npx".to_string()
}

pub struct McpManifestParser;

impl McpManifestParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Option<McpManifest> {
        let content = std::fs::read_to_string(path).ok()?;
        
        // 1. Prefer pure package.json
        let val = serde_json::from_str::<serde_json::Value>(&content).ok()?;
        let mut manifest = McpManifest::default();
        if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
            manifest.name = id.to_string();
        } else if let Some(n) = val.get("name").and_then(|v| v.as_str()) {
            manifest.name = n.to_string();
        }
        if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
            manifest.description = desc.to_string();
        }
        
        let mut exec = McpExecution::default();
        if let Some(versions) = val.get("versions").and_then(|v| v.as_object()) {
            if let Some(first_ver) = versions.values().next() {
                if let Some(cmd) = first_ver.get("command").and_then(|c| c.as_str()) {
                    exec.command = cmd.to_string();
                }
                if let Some(args_arr) = first_ver.get("args").and_then(|a| a.as_array()) {
                    exec.args = args_arr.iter().filter_map(|a| a.as_str().map(|s| s.to_string())).collect();
                }
                if let Some(env_obj) = first_ver.get("env").and_then(|e| e.as_object()) {
                    for (k, v) in env_obj {
                        if let Some(val_str) = v.as_str() {
                            exec.env.insert(k.clone(), val_str.to_string());
                        }
                    }
                }
            }
        }
        if exec.command.is_empty() {
            if let Some(cmd) = val.get("command").and_then(|c| c.as_str()) {
                exec.command = cmd.to_string();
            }
        }
        manifest.execution = Some(exec);
        Some(manifest)
    }
}
