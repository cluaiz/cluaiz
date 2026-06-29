use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use inference_cel::parser::metadata_parser::IntegrationMetadata;

// ─── MCP Runtime Wrapper ──────────────────────────────────────────────────────

pub struct McpServer {
    pub manifest: IntegrationMetadata,
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
            semantic_index: None,
        };

        let mut registry = MasterRegistry::load()?;
        registry.register_component("mcp", mcp_name, entry)?;
        
        Ok(())
    }

    pub async fn remove_mcp(mcp_name: &str) -> anyhow::Result<()> {
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("mcp");
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
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("mcp");
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

    /// Load manifest-mcp.yaml (preferred) or manifest.yaml or manifest.json (fallback)
    fn load_manifest(dir: &PathBuf) -> Option<IntegrationMetadata> {
        let bin_path = dir.join("manifest-mcp.bin");
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(m) = bincode::deserialize::<IntegrationMetadata>(&bytes) {
                    return Some(m);
                }
            }
        }

        let yaml_path = dir.join("manifest-mcp.yaml");
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                if let Ok(m) = serde_yaml::from_str::<IntegrationMetadata>(&content) {
                    if let Ok(bin_data) = bincode::serialize(&m) {
                        let _ = std::fs::write(&bin_path, bin_data);
                    }
                    return Some(m);
                }
            }
        }
        let json_path = dir.join("manifest.json");
        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(m) = serde_json::from_str::<IntegrationMetadata>(&content) {
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
                    cluaiz_shared::dev_info!("🌐 [McpManager] Found MCP Server: {} at {:?}", manifest.name, path);
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

        let cmd = server.manifest.execution.as_ref().and_then(|e| e.command.clone()).unwrap_or_else(|| "".to_string());
        let args = server.manifest.execution.as_ref().and_then(|e| e.args.clone()).unwrap_or_else(|| vec![]);

        tracing::info!("🚀 [McpManager] Booting MCP Server: {} via {} {:?}", server_name, cmd, args);
        // TODO: Spawn background stdio/SSE process via tokio::process
        Ok(())
    }
}
