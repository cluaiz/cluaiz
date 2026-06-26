use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::neural_foundry::registry::extension_manager::{AiInterface, EngineRules, StorageConfig};

// ─── MCP-Specific Execution Config ───────────────────────────────────────────
// MCP Servers are protocol bridges — they run as external processes
// (e.g., `npx -y @modelcontextprotocol/server-github`)

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpExecution {
    /// Command to run (e.g., "npx", "python", "node")
    #[serde(default)]
    pub command: String,
    /// Arguments to pass (e.g., ["-y", "@modelcontextprotocol/server-github"])
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set for this process
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

// ─── Full MCP Manifest ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: String,

    // ── Backwards-compat fields ──
    #[serde(default)]
    pub storage_domain: String,
    #[serde(default)]
    pub execution_command: String,
    #[serde(default)]
    pub execution_args: Vec<String>,

    /// How to start this MCP server (new standard field)
    #[serde(default)]
    pub execution: McpExecution,

    /// AI interface: keywords and CEL syntax for model routing
    #[serde(default)]
    pub ai_interface: Option<AiInterface>,

    /// Engine rules (mostly for network/process permissions)
    #[serde(default)]
    pub engine_rules: EngineRules,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
}

// ─── MCP Runtime Wrapper ──────────────────────────────────────────────────────

pub struct McpServer {
    pub manifest: McpManifest,
    pub path: PathBuf,
}

pub struct McpManager {
    pub active_servers: Vec<McpServer>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            active_servers: Vec::new(),
        }
    }

    pub async fn install_mcp(mcp_name: &str) -> anyhow::Result<()> {
        // 1. TODO: Download actual files from hub
        tracing::info!("⬇️ [McpManager] MCP files downloaded for {}", mcp_name);

        // 2. Write to registry.yaml
        use crate::neural_foundry::registry::registry_index::{MasterRegistry, RegistryEntry, LoadStrategy};
        let domain = format!("mcp/{}", mcp_name);
        let entry = RegistryEntry {
            id: format!("mcp_{}_{}", mcp_name, chrono::Utc::now().timestamp()),
            domain,
            load_strategy: LoadStrategy::Lazy,
            activation_events: vec![
                format!("on_command:use mcp::{}", mcp_name),
            ],
            enabled: true,
            binary_hash: None,
        };

        let mut registry = MasterRegistry::load()?;
        registry.register_component("mcp", mcp_name, entry)?;
        
        Ok(())
    }

    pub async fn remove_mcp(mcp_name: &str) -> anyhow::Result<()> {
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("mcp");
        let mut found_path = None;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.file_name().unwrap_or_default() == mcp_name {
                    found_path = Some(path);
                    break;
                }
            }
        }
        
        if let Some(path) = found_path {
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_dir_all(&path);
            }).await?;
            
            // Remove from registry.yaml
            use crate::neural_foundry::registry::registry_index::MasterRegistry;
            if let Ok(mut registry) = MasterRegistry::load() {
                let _ = registry.deregister_component("mcp", mcp_name);
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("MCP Server '{}' not found on disk", mcp_name))
        }
    }

    pub async fn clear_mcp_cache(mcp_name: Option<&str>) -> anyhow::Result<usize> {
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("mcp");
        let mut wiped = 0;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if let Some(m_name) = mcp_name {
                    if path.file_name().unwrap_or_default() != m_name { continue; }
                }
                let cache_dir = path.join(".cache");
                if cache_dir.exists() {
                    let _ = std::fs::remove_dir_all(&cache_dir);
                    wiped += 1;
                }
            }
        }
        Ok(wiped)
    }

    /// Load manifest.yaml (preferred) or manifest.json (fallback)
    fn load_manifest(dir: &PathBuf) -> Option<McpManifest> {
        let yaml_path = dir.join("manifest.yaml");
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                if let Ok(m) = serde_yaml::from_str::<McpManifest>(&content) {
                    return Some(m);
                }
            }
        }
        let json_path = dir.join("manifest.json");
        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(m) = serde_json::from_str::<McpManifest>(&content) {
                    return Some(m);
                }
            }
        }
        None
    }

    /// Dynamically load MCP configs from a given domain path (YAML-first)
    pub fn scan_domain(&mut self, base_domain_path: &PathBuf) -> Result<()> {
        if !base_domain_path.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(base_domain_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(manifest) = Self::load_manifest(&path) {
                    cluaize_shared::dev_info!("🌐 [McpManager] Found MCP Server: {} at {:?}", manifest.name, path);
                    self.active_servers.push(McpServer { manifest, path });
                }
            }
        }
        Ok(())
    }

    /// Start an MCP server as a background daemon process
    pub fn start_server(&self, server_name: &str) -> Result<()> {
        let server = self.active_servers.iter().find(|s| s.manifest.name == server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP Server '{}' not found", server_name))?;

        let cache_dir = server.path.join(".cache");
        if !cache_dir.exists() {
            let _ = std::fs::create_dir_all(&cache_dir);
        }

        // Prefer new `execution` field, fallback to legacy fields
        let cmd = if !server.manifest.execution.command.is_empty() {
            server.manifest.execution.command.clone()
        } else {
            server.manifest.execution_command.clone()
        };
        let args = if !server.manifest.execution.args.is_empty() {
            server.manifest.execution.args.clone()
        } else {
            server.manifest.execution_args.clone()
        };

        tracing::info!("🚀 [McpManager] Booting MCP Server: {} via {} {:?}", server_name, cmd, args);
        // TODO: Spawn background stdio/SSE process via tokio::process
        Ok(())
    }
}
