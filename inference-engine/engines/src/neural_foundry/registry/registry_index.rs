// cluaiz-engine: Core Foundry - Registry Index
// O(1) Master Registry that replaces filesystem walks at runtime.
// Stored as:
//   - ~/.cluaiz/engine/config/registry.yaml  (Human readable / editable)
//   - ~/.cluaiz/engine/config/registry.bin   (Bincode pre-compiled, 0-ms load)

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Result;

// ─── Entry Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LoadStrategy {
    Eager,
    Lazy,
}

impl Default for LoadStrategy {
    fn default() -> Self {
        LoadStrategy::Lazy
    }
}

/// Execution mode for installed tools and skills
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Auto,
    Manual,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        ExecutionMode::Auto
    }
}

/// A single entry in the registry representing an installed component.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RegistryEntry {
    /// Unique identifier e.g., "cluaiz-search"
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub id: String,

    #[serde(default)]
    pub domain: String,

    /// Semantic version
    #[serde(default)]
    pub version: String,

    /// Short description
    #[serde(default)]
    pub description: String,

    /// How the engine loads this component
    #[serde(default)]
    pub load_strategy: LoadStrategy,

    /// Words/phrases that trigger this component to be loaded
    #[serde(default)]
    pub semantic_triggers: Vec<String>,

    #[serde(default)]
    pub semantic_index: Vec<String>,

    /// Specific event strings like "on_command:use plugin::math"
    #[serde(default)]
    pub trigger_events: Vec<String>,

    #[serde(default)]
    pub activation_events: Vec<String>,

    /// Path to the component folder relative to ~/.cluaiz/
    /// e.g., "plugins/cluaiz-search"
    #[serde(default)]
    pub location: String,

    /// Path to the compiled binary/wasm (if applicable)
    #[serde(default)]
    pub binary: Option<String>,

    #[serde(default)]
    pub binary_hash: Option<String>,

    /// Path to SKILL.md (if applicable)
    #[serde(default)]
    pub brain: Option<String>,

    /// Whether this component is enabled in the registry
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Execution mode: Auto (runs on trigger) or Manual (requires approval)
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Granular permissions e.g. ["fs:read", "net:fetch"]
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_true() -> bool {
    true
}

// ─── Master Registry (Deserializes the full registry.yaml) ───────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MasterRegistry {
    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub last_updated: String,

    #[serde(default = "default_schema")]
    pub schema: String,

    /// Plugins (WASM or Native tools, stored in ~/.cluaiz/plugins/)
    #[serde(default)]
    pub plugins: HashMap<String, RegistryEntry>,

    /// MCP servers (Protocol bridges, stored in ~/.cluaiz/mcp/)
    #[serde(default)]
    pub mcp: HashMap<String, RegistryEntry>,

    /// Skills (Cognitive SKILL.md tools, stored in ~/.cluaiz/skills/)
    #[serde(default)]
    pub skills: HashMap<String, RegistryEntry>,
}

fn default_version() -> String { "1.0.0".to_string() }
fn default_schema() -> String { "cluaiz-registry-v1".to_string() }

impl MasterRegistry {
    /// Returns the canonical path for registry.yaml
    /// Location: ~/.cluaiz/engine/config/registry.yaml
    pub fn registry_path() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current()
            .config_dir()
            .join("registry.yaml")
    }

    /// Returns the canonical path for the pre-compiled binary cache
    /// Location: ~/.cluaiz/engine/config/registry.bin
    pub fn registry_bin_path() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current()
            .config_dir()
            .join("registry.bin")
    }

    /// Load the registry.
    /// Fast Path:  Tries ~/.cluaiz/engine/config/registry.bin (Bincode deserialization).
    /// Slow Path:  Falls back to registry.yaml if .bin does not exist or is corrupted.
    /// Cold Path:  Returns empty MasterRegistry if neither exists.
    /// Called ONCE at engine cold boot.
    pub fn load() -> Result<Self> {
        let bin_path = Self::registry_bin_path();
        let yaml_path = Self::registry_path();

        // 1. Try fast binary cache first
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(mut registry) = bincode::deserialize::<MasterRegistry>(&bytes) {
                    let _ = registry.sync_with_filesystem();
                    tracing::info!("📋 [Registry] Loaded {} plugins, {} mcp from binary cache",
                        registry.plugins.len(),
                        registry.mcp.len()
                    );
                    return Ok(registry);
                }
            }
        }

        // 2. Fall back to YAML source
        if yaml_path.exists() {
            let content = std::fs::read_to_string(&yaml_path)?;
            let mut registry: MasterRegistry = serde_yaml::from_str(&content)?;
            let _ = registry.sync_with_filesystem();

            tracing::info!("📋 [Registry] Loaded {} plugins, {} mcp from registry.yaml",
                registry.plugins.len(),
                registry.mcp.len()
            );

            // Write binary cache for next boot
            registry.save_bin_cache()?;
            return Ok(registry);
        }

        // 3. First run — return empty registry
        tracing::info!("📋 [Registry] No registry.yaml found. Starting with empty registry.");
        let registry = MasterRegistry::default();
        let _ = registry.save();
        Ok(registry)
    }

    /// Save the current in-memory registry back to registry.yaml (human-readable)
    pub fn save(&self) -> Result<()> {
        let yaml_path = Self::registry_path();
        if let Some(parent) = yaml_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Update last_updated timestamp
        let mut updated = self.clone();
        updated.last_updated = chrono::Utc::now().to_rfc3339();

        let yaml_str = serde_yaml::to_string(&updated)?;
        std::fs::write(&yaml_path, yaml_str)?;

        // Regenerate binary cache
        updated.save_bin_cache()?;

        tracing::info!("💾 [Registry] Saved registry.yaml with {} plugins, {} mcp",
            self.plugins.len(), self.mcp.len());
        Ok(())
    }

    /// Write binary cache (registry.bin) for zero-copy fast boot
    fn save_bin_cache(&self) -> Result<()> {
        let bin_path = Self::registry_bin_path();
        if let Some(parent) = bin_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = bincode::serialize(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize registry to binary: {}", e))?;
        std::fs::write(&bin_path, bytes)?;
        Ok(())
    }

    pub fn sync_with_filesystem(&mut self) -> Result<()> {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let plugins_dir = env.plugins_dir();
        let mcp_dir = env.mcp_dir();
        let skills_dir = env.skills_dir();
        
        let mut changed = false;
        
        // plugins
        let plugin_keys: Vec<String> = self.plugins.keys().cloned().collect();
        for key in plugin_keys {
            if !plugins_dir.join(&key).exists() {
                self.plugins.remove(&key);
                changed = true;
            }
        }
        
        // mcp
        let mcp_keys: Vec<String> = self.mcp.keys().cloned().collect();
        for key in mcp_keys {
            if !mcp_dir.join(&key).exists() {
                self.mcp.remove(&key);
                changed = true;
            }
        }

        // skills
        let skill_keys: Vec<String> = self.skills.keys().cloned().collect();
        for key in skill_keys {
            if !skills_dir.join(&key).exists() {
                self.skills.remove(&key);
                changed = true;
            }
        }
        
        if changed {
            let _ = self.save();
        }
        
        Ok(())
    }

    /// Add/update a component entry and persist to disk.
    /// Called after `cluaiz plugin/skill install <name>`.
    pub fn register_component(&mut self, component_type: &str, name: &str, entry: RegistryEntry) -> Result<()> {
        match component_type {
            "plugin" | "plugins" => { self.plugins.insert(name.to_string(), entry); }
            "mcp"                => { self.mcp.insert(name.to_string(), entry); }
            "skill" | "skills"   => { self.skills.insert(name.to_string(), entry); }
            other => return Err(anyhow::anyhow!("Unknown component type: {}", other)),
        }
        self.save()
    }

    /// Remove a component entry and persist to disk.
    /// Called after `cluaiz plugin/skill remove <name>`.
    pub fn deregister_component(&mut self, component_type: &str, name: &str) -> Result<()> {
        let removed = match component_type {
            "plugin" | "plugins" => self.plugins.remove(name).is_some(),
            "mcp"                => self.mcp.remove(name).is_some(),
            "skill" | "skills"   => self.skills.remove(name).is_some(),
            other => return Err(anyhow::anyhow!("Unknown component type: {}", other)),
        };

        if !removed {
            return Err(anyhow::anyhow!("Component '{}' not found in registry", name));
        }

        self.save()
    }

    /// Toggle enabled/disabled state of a component
    pub fn set_enabled(&mut self, component_type: &str, name: &str, enabled: bool) -> Result<()> {
        let entry = match component_type {
            "plugin" | "plugins" => self.plugins.get_mut(name),
            "mcp"                => self.mcp.get_mut(name),
            "skill" | "skills"   => self.skills.get_mut(name),
            other => return Err(anyhow::anyhow!("Unknown component type: {}", other)),
        };

        match entry {
            Some(e) => {
                e.enabled = enabled;
                self.save()
            }
            None => Err(anyhow::anyhow!("Component '{}' not found in registry", name)),
        }
    }

    /// Set execution mode (Auto or Manual) for a component
    pub fn set_execution_mode(&mut self, component_type: &str, name: &str, mode: ExecutionMode) -> Result<()> {
        let entry = match component_type {
            "plugin" | "plugins" => self.plugins.get_mut(name),
            "mcp"                => self.mcp.get_mut(name),
            "skill" | "skills"   => self.skills.get_mut(name),
            other => return Err(anyhow::anyhow!("Unknown component type: {}", other)),
        };

        match entry {
            Some(e) => {
                e.execution_mode = mode;
                self.save()
            }
            None => Err(anyhow::anyhow!("Component '{}' not found in registry", name)),
        }
    }

    /// Get all EAGER components across all types that are enabled.
    /// Called at boot to know what to load immediately.
    pub fn eager_components(&self) -> Vec<(String, &str, &RegistryEntry)> {
        let mut result = Vec::new();
        for (name, entry) in &self.plugins {
            if entry.enabled && entry.load_strategy == LoadStrategy::Eager {
                result.push((name.clone(), "plugin", entry));
            }
        }
        for (name, entry) in &self.mcp {
            if entry.enabled && entry.load_strategy == LoadStrategy::Eager {
                result.push((name.clone(), "mcp", entry));
            }
        }
        result
    }

    /// Get all LAZY components that are enabled, with their component types.
    /// Called at boot to populate the ActivationEventBus (zero RAM cost).
    pub fn lazy_watch_list(&self) -> Vec<(String, &str, &RegistryEntry)> {
        let mut result = Vec::new();
        for (name, entry) in &self.plugins {
            if entry.enabled && entry.load_strategy == LoadStrategy::Lazy {
                result.push((name.clone(), "plugin", entry));
            }
        }
        for (name, entry) in &self.mcp {
            if entry.enabled && entry.load_strategy == LoadStrategy::Lazy {
                result.push((name.clone(), "mcp", entry));
            }
        }
        result
    }

    /// List all components with their status (for `cluaiz plugin list` CLI)
    pub fn list_all(&self) -> Vec<(String, &str, &RegistryEntry)> {
        let mut result = Vec::new();
        for (name, entry) in &self.plugins {
            result.push((name.clone(), "plugin", entry));
        }
        for (name, entry) in &self.mcp {
            result.push((name.clone(), "mcp", entry));
        }
        result
    }
}
