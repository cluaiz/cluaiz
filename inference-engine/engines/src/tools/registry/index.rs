use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use cluaiz_shared::environment::EnvironmentManager;
use super::types::{ExecutionMode, SecurityMode, ToolEntry};

/// Master Tools Registry (Single Source of Truth for all Skills, Plugins, and MCP connectors)
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ToolsRegistry {
    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub last_updated: String,

    /// Global default security mode ("full_access" | "sandboxed" | "strict")
    #[serde(default)]
    pub default_security_mode: SecurityMode,

    /// Map of tool_id -> ToolEntry
    #[serde(default)]
    pub installed_tools: HashMap<String, ToolEntry>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl ToolsRegistry {
    /// Returns canonical path to `tools_registry.json` (~/.cluaiz/engine/config/tools_registry.json)
    pub fn registry_path() -> PathBuf {
        EnvironmentManager::current().tools_registry_json_path()
    }

    /// Returns canonical path to binary cache `tools_registry.bin` (~/.cluaiz/engine/config/tools_registry.bin)
    pub fn registry_bin_path() -> PathBuf {
        EnvironmentManager::current().tools_registry_bin_path()
    }

    /// Load the tools registry.
    /// Fast Path: Tries ~/.cluaiz/engine/config/tools_registry.bin (Bincode deserialization, 0-ms load).
    /// Slow Path: Falls back to tools_registry.json or legacy registry.yaml if .bin does not exist.
    /// Cold Path: Auto-probes filesystem and creates new registry.
    pub fn load() -> Result<Self> {
        let bin_path = Self::registry_bin_path();
        let json_path = Self::registry_path();
        let env = EnvironmentManager::current();

        // 1. Fast binary cache path
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(mut reg) = bincode::deserialize::<ToolsRegistry>(&bytes) {
                    let _ = reg.sync_with_filesystem();
                    return Ok(reg);
                }
            }
        }

        // 2. Standard JSON path
        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(mut reg) = serde_json::from_str::<ToolsRegistry>(&content) {
                    let _ = reg.sync_with_filesystem();
                    let _ = reg.save_bin_cache();
                    return Ok(reg);
                }
            }
        }

        // 3. Fallback: Legacy migration from registry.yaml
        let legacy_yaml_path = env.config_dir().join("registry.yaml");
        if legacy_yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&legacy_yaml_path) {
                if let Ok(legacy_val) = serde_yaml::from_str::<serde_json::Value>(&content) {
                    let mut reg = ToolsRegistry::default();
                    // Migrate plugins
                    if let Some(plugins) = legacy_val.get("plugins").and_then(|p| p.as_object()) {
                        for (k, v) in plugins {
                            reg.installed_tools.insert(k.clone(), ToolEntry {
                                id: k.clone(),
                                name: v.get("name").and_then(|n| n.as_str()).unwrap_or(k).to_string(),
                                category: "plugin".to_string(),
                                version: v.get("version").and_then(|ver| ver.as_str()).unwrap_or("1.0.0").to_string(),
                                description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                local_dir: env.plugins_dir().join(k).to_string_lossy().to_string(),
                                binary_path: v.get("binary").and_then(|b| b.as_str()).map(|s| s.to_string()),
                                enabled: v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                                security_mode: SecurityMode::FullAccess,
                                execution_mode: ExecutionMode::Auto,
                                default_turns: -1,
                                permissions: Vec::new(),
                                semantic_triggers: Vec::new(),
                                activation_events: Vec::new(),
                            });
                        }
                    }
                    // Migrate skills
                    if let Some(skills) = legacy_val.get("skills").and_then(|s| s.as_object()) {
                        for (k, v) in skills {
                            reg.installed_tools.insert(k.clone(), ToolEntry {
                                id: k.clone(),
                                name: v.get("name").and_then(|n| n.as_str()).unwrap_or(k).to_string(),
                                category: "skill".to_string(),
                                version: v.get("version").and_then(|ver| ver.as_str()).unwrap_or("1.0.0").to_string(),
                                description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                local_dir: env.skills_dir().join(k).to_string_lossy().to_string(),
                                binary_path: None,
                                enabled: v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                                security_mode: SecurityMode::FullAccess,
                                execution_mode: ExecutionMode::Auto,
                                default_turns: -1,
                                permissions: Vec::new(),
                                semantic_triggers: Vec::new(),
                                activation_events: Vec::new(),
                            });
                        }
                    }
                    // Migrate mcp
                    if let Some(mcp) = legacy_val.get("mcp").and_then(|m| m.as_object()) {
                        for (k, v) in mcp {
                            reg.installed_tools.insert(k.clone(), ToolEntry {
                                id: k.clone(),
                                name: v.get("name").and_then(|n| n.as_str()).unwrap_or(k).to_string(),
                                category: "mcp".to_string(),
                                version: v.get("version").and_then(|ver| ver.as_str()).unwrap_or("1.0.0").to_string(),
                                description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                local_dir: env.mcp_dir().join(k).to_string_lossy().to_string(),
                                binary_path: None,
                                enabled: v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                                security_mode: SecurityMode::Strict,
                                execution_mode: ExecutionMode::Manual,
                                default_turns: 3,
                                permissions: Vec::new(),
                                semantic_triggers: Vec::new(),
                                activation_events: Vec::new(),
                            });
                        }
                    }

                    let _ = reg.sync_with_filesystem();
                    let _ = reg.save();
                    tracing::info!("✅ Successfully migrated legacy registry.yaml into tools_registry.json");
                    return Ok(reg);
                }
            }
        }

        // 4. Clean Cold boot
        let mut reg = ToolsRegistry::default();
        let _ = reg.sync_with_filesystem();
        let _ = reg.save();
        Ok(reg)
    }

    /// Save the registry to `tools_registry.json` and sync the binary cache
    pub fn save(&self) -> Result<()> {
        let json_path = Self::registry_path();
        if let Some(parent) = json_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut updated = self.clone();
        updated.last_updated = chrono::Utc::now().to_rfc3339();

        let json_str = serde_json::to_string_pretty(&updated)?;
        std::fs::write(&json_path, json_str)?;

        // Update fast binary cache
        updated.save_bin_cache()?;
        Ok(())
    }

    /// Save fast binary cache for 0-ms cold boots
    pub fn save_bin_cache(&self) -> Result<()> {
        let bin_path = Self::registry_bin_path();
        if let Some(parent) = bin_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = bincode::serialize(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize tools_registry to binary: {}", e))?;
        std::fs::write(&bin_path, bytes)?;
        Ok(())
    }

    /// Returns a specific tool entry by ID
    pub fn get_tool(&self, id: &str) -> Option<&ToolEntry> {
        self.installed_tools.get(id)
    }

    /// Returns all registered tools
    pub fn list_tools(&self) -> Vec<ToolEntry> {
        self.installed_tools.values().cloned().collect()
    }

    /// Set a tool's enabled switch and persist changes
    pub fn set_tool_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.enabled = enabled;
            self.save()?;
        }
        Ok(())
    }

    /// Set a tool's execution mode (Auto/Manual) and persist changes
    pub fn set_tool_execution_mode(&mut self, id: &str, mode: ExecutionMode) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.execution_mode = mode;
            self.save()?;
        }
        Ok(())
    }

    /// Set a tool's default turns and persist changes
    pub fn set_tool_default_turns(&mut self, id: &str, turns: i32) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.default_turns = turns;
            self.save()?;
        }
        Ok(())
    }
}
