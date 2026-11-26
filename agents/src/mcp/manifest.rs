use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
pub struct McpExecution {
    #[serde(default = "default_command")]
    pub command: String, // "npx", "node", "python", "uvx"
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// An HTTP server: the MCP endpoint (Streamable HTTP). When set, `command` and `args` are not used.
    #[serde(default)]
    pub url: Option<String>,
    /// Headers sent with every request to `url`; `${NAME}` in a value is replaced by the setting `NAME`
    #[serde(default)]
    pub headers: HashMap<String, String>,
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
        if let Some(exec_obj) = val.get("execution").and_then(|e| e.as_object()) {
            if let Some(cmd) = exec_obj.get("command").and_then(|c| c.as_str()) {
                exec.command = cmd.to_string();
            }
            if let Some(args_arr) = exec_obj.get("args").and_then(|a| a.as_array()) {
                exec.args = args_arr
                    .iter()
                    .filter_map(|a| a.as_str().map(|s| s.to_string()))
                    .collect();
            }
            if let Some(env_obj) = exec_obj.get("env").and_then(|e| e.as_object()) {
                for (k, v) in env_obj {
                    if let Some(val_str) = v.as_str() {
                        exec.env.insert(k.clone(), val_str.to_string());
                    }
                }
            }
            exec.url = exec_obj.get("url").and_then(|u| u.as_str()).map(str::trim).filter(|u| !u.is_empty()).map(str::to_string);
            if let Some(headers) = exec_obj.get("headers").and_then(|h| h.as_object()) {
                for (k, v) in headers {
                    if let Some(val_str) = v.as_str() {
                        exec.headers.insert(k.clone(), val_str.to_string());
                    }
                }
            }
        }
        // A url means an HTTP server: the stdio fallbacks below do not apply
        if exec.command.is_empty() && exec.url.is_none() {
            if let Some(versions) = val.get("versions").and_then(|v| v.as_object()) {
                if let Some(first_ver) = versions.values().next() {
                    if let Some(cmd) = first_ver.get("command").and_then(|c| c.as_str()) {
                        exec.command = cmd.to_string();
                    }
                    if let Some(args_arr) = first_ver.get("args").and_then(|a| a.as_array()) {
                        exec.args = args_arr
                            .iter()
                            .filter_map(|a| a.as_str().map(|s| s.to_string()))
                            .collect();
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
        }
        if exec.command.is_empty() && exec.url.is_none() {
            if let Some(cmd) = val.get("command").and_then(|c| c.as_str()) {
                exec.command = cmd.to_string();
            }
        }
        manifest.execution = Some(exec);
        Some(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> McpExecution {
        let path = std::env::temp_dir().join(format!("cluaiz-manifest-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, json).unwrap();
        let manifest = McpManifestParser::parse_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        manifest.execution.unwrap()
    }

    #[test]
    fn url_form_and_command_form_are_told_apart() {
        let http = parse(r#"{"id":"a","execution":{"url":"https://x.test/mcp","headers":{"Authorization":"Bearer ${TOKEN}"}}}"#);
        assert_eq!(http.url.as_deref(), Some("https://x.test/mcp"));
        assert_eq!(http.headers["Authorization"], "Bearer ${TOKEN}");
        assert!(http.command.is_empty());

        let stdio = parse(r#"{"id":"b","execution":{"command":"npx","args":["-y","x"]}}"#);
        assert!(stdio.url.is_none());
        assert_eq!(stdio.command, "npx");

        // An empty url is no url
        assert!(parse(r#"{"id":"c","execution":{"url":"  ","command":"node"}}"#).url.is_none());
    }
}
