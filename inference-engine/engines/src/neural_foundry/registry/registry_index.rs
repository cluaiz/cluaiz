use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Result;

// ─── Load Strategy ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum LoadStrategy {
    /// Load binary into RAM at engine startup (for core dependencies like cluaiz-db)
    Eager,
    /// Register activation events only. Zero RAM until triggered at runtime.
    Lazy,
    /// Never auto-load. Only via explicit `cluaiz load <name>` CLI command.
    Manual,
}

impl Default for LoadStrategy {
    fn default() -> Self {
        LoadStrategy::Lazy
    }
}

// ─── Registry Entry (One per component in registry.yaml) ─────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryEntry {
    /// Unique ID for this component (e.g., "ext_core_db_001")
    pub id: String,

    /// Relative path under ~/.cluaiz/ where this component lives
    /// e.g., "extension/cluaiz-db" or "plugin/web-scraper"
    pub domain: String,

    /// Whether to load at startup (Eager) or only on trigger (Lazy)
    #[serde(default)]
    pub load_strategy: LoadStrategy,

    /// Events that trigger this component to be lazy-loaded
    /// Formats: "on_startup", "on_cel_keyword:scrape", "on_command:use plugin::web-scraper"
    #[serde(default)]
    pub activation_events: Vec<String>,

    /// Whether this component is active. Disabled = never loaded, even if triggered.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// SHA256 hash of the compiled binary for integrity verification
    /// Format: "sha256:a4fbc89e3fbc..."
    pub binary_hash: Option<String>,

    /// Global lookup index for semantic triggers (e.g. ["math", "calculator"])
    /// Used by the Engine to instantly find extensions without parsing manifests.
    #[serde(default)]
    pub semantic_index: Option<Vec<String>>,
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

    /// Extensions (Brain + Muscle bundles, stored in ~/.cluaiz/extension/)
    #[serde(default)]
    pub extensions: HashMap<String, RegistryEntry>,

    /// Plugins (Pure tool .dll, stored in ~/.cluaiz/plugin/)
    #[serde(default)]
    pub plugins: HashMap<String, RegistryEntry>,

    /// MCP servers (Protocol bridges, stored in ~/.cluaiz/mcp/)
    #[serde(default)]
    pub mcp: HashMap<String, RegistryEntry>,
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

    /// Returns the canonical path for registry.bin (binary cache)
    pub fn registry_bin_path() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current()
            .config_dir()
            .join("registry.bin")
    }

    /// Load MasterRegistry from disk.
    /// Priority: registry.bin (fast binary) → registry.yaml (source of truth)
    /// Called ONCE at engine cold boot.
    pub fn load() -> Result<Self> {
        let bin_path = Self::registry_bin_path();
        let yaml_path = Self::registry_path();

        // 1. Try fast binary cache first
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(mut registry) = bincode::deserialize::<MasterRegistry>(&bytes) {
                    let _ = registry.sync_with_filesystem();
                    tracing::info!("📋 [Registry] Loaded {} extensions, {} plugins, {} mcp from binary cache",
                        registry.extensions.len(),
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

            tracing::info!("📋 [Registry] Loaded {} extensions, {} plugins, {} mcp from registry.yaml",
                registry.extensions.len(),
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

        tracing::info!("💾 [Registry] Saved registry.yaml with {} extensions, {} plugins, {} mcp",
            self.extensions.len(), self.plugins.len(), self.mcp.len());
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
        let ext_dir = env.extensions_dir();
        let plugins_dir = env.plugins_dir();
        let mcp_dir = env.mcp_dir();
        let skills_dir = env.skills_dir();
        
        let mut changed = false;
        
        // extensions
        let ext_keys: Vec<String> = self.extensions.keys().cloned().collect();
        for key in ext_keys {
            if !ext_dir.join(&key).exists() && !skills_dir.join(&key).exists() {
                self.extensions.remove(&key);
                changed = true;
            }
        }
        
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
        
        if changed {
            let _ = self.save();
        }
        
        Ok(())
    }

    /// Add/update a component entry and persist to disk.
    /// Called by CLI after `cluaiz extension install <name>`.
    pub fn register_component(&mut self, component_type: &str, name: &str, entry: RegistryEntry) -> Result<()> {
        match component_type {
            "extension" | "extensions" => { self.extensions.insert(name.to_string(), entry); }
            "plugin"    | "plugins"    => { self.plugins.insert(name.to_string(), entry); }
            "mcp"                      => { self.mcp.insert(name.to_string(), entry); }
            other => return Err(anyhow::anyhow!("Unknown component type: {}", other)),
        }
        self.save()
    }

    /// Remove a component entry and persist to disk.
    /// Called by CLI after `cluaiz extension remove <name>`.
    pub fn deregister_component(&mut self, component_type: &str, name: &str) -> Result<()> {
        let removed = match component_type {
            "extension" | "extensions" => self.extensions.remove(name).is_some(),
            "plugin"    | "plugins"    => self.plugins.remove(name).is_some(),
            "mcp"                      => self.mcp.remove(name).is_some(),
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
            "extension" | "extensions" => self.extensions.get_mut(name),
            "plugin"    | "plugins"    => self.plugins.get_mut(name),
            "mcp"                      => self.mcp.get_mut(name),
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

    /// Get all EAGER components across all types that are enabled.
    /// Called at boot to know what to load immediately.
    pub fn eager_components(&self) -> Vec<(String, &str, &RegistryEntry)> {
        let mut result = Vec::new();
        for (name, entry) in &self.extensions {
            if entry.enabled && entry.load_strategy == LoadStrategy::Eager {
                result.push((name.clone(), "extension", entry));
            }
        }
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
        for (name, entry) in &self.extensions {
            if entry.enabled && entry.load_strategy == LoadStrategy::Lazy {
                result.push((name.clone(), "extension", entry));
            }
        }
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

    /// List all components with their status (for `cluaiz extension list` CLI)
    pub fn list_all(&self) -> Vec<(String, &str, &RegistryEntry)> {
        let mut result = Vec::new();
        for (name, entry) in &self.extensions {
            result.push((name.clone(), "extension", entry));
        }
        for (name, entry) in &self.plugins {
            result.push((name.clone(), "plugin", entry));
        }
        for (name, entry) in &self.mcp {
            result.push((name.clone(), "mcp", entry));
        }
        result
    }
}
