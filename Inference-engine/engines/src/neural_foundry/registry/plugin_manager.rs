use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::neural_foundry::registry::extension_manager::{AiInterface, EngineRules, FfiBindings, StorageConfig};

// ─── Full Plugin Manifest ─────────────────────────────────────────────────────
// Plugins are pure muscle (tool .dll/WASM) — no knowledge/brain component.
// They live in ~/.cluaize/tools/<plugin_name>/

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: String,

    /// Storage domain — kept for backwards compat
    #[serde(default)]
    pub storage_domain: String,
    /// Backwards-compat field
    #[serde(default)]
    pub native_binary: String,

    /// AI interface: keywords and CEL syntax for model routing
    #[serde(default)]
    pub ai_interface: Option<AiInterface>,

    /// Engine rules: hardware limits and permissions
    #[serde(default)]
    pub engine_rules: EngineRules,

    /// FFI bindings: path to the binary and entry point
    #[serde(default)]
    pub ffi_bindings: FfiBindings,

    /// New Schema: Execution definitions
    #[serde(default)]
    pub execution: Option<serde_json::Value>,

    /// New Schema: Security permissions
    #[serde(default)]
    pub permissions: Option<serde_json::Value>,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
}

// ─── Plugin Runtime Wrapper ───────────────────────────────────────────────────

pub struct Plugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
}

pub struct PluginManager {
    pub active_plugins: Vec<Plugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            active_plugins: Vec::new(),
        }
    }

    pub async fn install_plugin(plugin_name: &str) -> anyhow::Result<()> {
        // 1. Dynamically resolve actual download URL from package.json and registry.json
        let hub_url = crate::neural_foundry::registry::download_manager::DownloadManager::resolve_hub_url("hub", plugin_name).await?;
        
        // 2. Download and extract the plugin files natively
        let _path = crate::neural_foundry::registry::download_manager::DownloadManager::download_and_extract(&hub_url, "tools", plugin_name).await?;
        
        tracing::info!("⬇️ [PluginManager] Plugin files downloaded for {} from {}", plugin_name, hub_url);

        // 3. 🛡️ Run CEL Safety Checker (4-Step Audit) BEFORE registration
        let manifest_path = _path.join("manifest.yaml");
        let manifest_json_path = _path.join("manifest.json");
        let active_manifest_path = if manifest_path.exists() { &manifest_path } else { &manifest_json_path };
        
        if active_manifest_path.exists() {
            let content = std::fs::read_to_string(active_manifest_path)?;
            let manifest_val: serde_json::Value = if active_manifest_path.extension().unwrap_or_default() == "yaml" {
                serde_yaml::from_str(&content).map_err(|e| anyhow::anyhow!("YAML Parse error: {}", e))?
            } else {
                serde_json::from_str(&content)?
            };
            
            let binary_name = manifest_val["ffi_bindings"]["binary_path"]
                .as_str()
                .unwrap_or_else(|| manifest_val["native_binary"].as_str().unwrap_or(""));
            let binary_path = _path.join(binary_name);
            
            inference_cel::execution::safety_checker::SafetyChecker::audit_plugin(active_manifest_path, &binary_path, &manifest_val)
                .map_err(|e| anyhow::anyhow!("Safety Audit Failed: {}", e))?;
        } else {
            return Err(anyhow::anyhow!("Safety Audit Failed: Invalid or missing manifest."));
        }

        // 4. Write to registry.yaml
        use crate::neural_foundry::registry::registry_index::{MasterRegistry, RegistryEntry, LoadStrategy};
        let domain = format!("tools/{}", plugin_name);
        let entry = RegistryEntry {
            id: format!("plugin_{}_{}", plugin_name, chrono::Utc::now().timestamp()),
            domain,
            load_strategy: LoadStrategy::Lazy,
            activation_events: vec![
                format!("on_command:use plugin::{}", plugin_name),
            ],
            enabled: true,
            binary_hash: None,
            semantic_index: None,
        };

        let mut registry = MasterRegistry::load()?;
        registry.register_component("plugin", plugin_name, entry)?;
        
        Ok(())
    }

    pub async fn remove_plugin(plugin_name: &str) -> anyhow::Result<()> {
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("tools");
        let mut found_path = None;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    for sub_entry in std::fs::read_dir(&path)? {
                        let sub_entry = sub_entry?;
                        let sub_path = sub_entry.path();
                        if sub_path.file_name().unwrap_or_default() == plugin_name {
                            found_path = Some(sub_path);
                            break;
                        }
                    }
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
                let _ = registry.deregister_component("plugin", plugin_name);
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin '{}' not found on disk", plugin_name))
        }
    }

    pub async fn clear_plugin_cache(plugin_name: Option<&str>) -> anyhow::Result<usize> {
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("tools");
        let mut wiped = 0;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    for sub_entry in std::fs::read_dir(&path)? {
                        let sub_entry = sub_entry?;
                        let sub_path = sub_entry.path();
                        if let Some(p_name) = plugin_name {
                            if sub_path.file_name().unwrap_or_default() != p_name { continue; }
                        }
                        let cache_dir = sub_path.join(".cache");
                        if cache_dir.exists() {
                            let _ = std::fs::remove_dir_all(&cache_dir);
                            wiped += 1;
                        }
                    }
                }
            }
        }
        Ok(wiped)
    }

    /// Load manifest from a plugin directory.
    /// Priority: manifest.yaml → manifest.json (backwards compat)
    fn load_manifest(dir: &PathBuf) -> Option<PluginManifest> {
        let yaml_path = dir.join("manifest.yaml");
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                if let Ok(m) = serde_yaml::from_str::<PluginManifest>(&content) {
                    return Some(m);
                }
            }
        }
        let json_path = dir.join("manifest.json");
        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(m) = serde_json::from_str::<PluginManifest>(&content) {
                    return Some(m);
                }
            }
        }
        None
    }

    /// Dynamically load plugins from a given domain path.
    /// Uses YAML-first manifest loading with JSON fallback.
    pub fn scan_domain(&mut self, base_domain_path: &PathBuf) -> Result<()> {
        if !base_domain_path.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(base_domain_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(manifest) = Self::load_manifest(&path) {
                    cluaize_shared::dev_info!("🔌 [PluginManager] Found Plugin Muscle: {} at {:?}", manifest.name, path);
                    self.active_plugins.push(Plugin { manifest, path });
                }
            }
        }
        Ok(())
    }

    /// Expose native binary path to a Skill or CEL caller
    pub fn get_plugin_binary_path(&self, plugin_name: &str) -> Option<PathBuf> {
        self.active_plugins.iter()
            .find(|p| p.manifest.name == plugin_name)
            .map(|p| {
                // Prefer new schema execution.binary_path, then ffi_bindings.binary_path, fall back to native_binary
                let binary = if let Some(exec) = &p.manifest.execution {
                    if let Some(bp) = exec.get("binary_path").and_then(|v| v.as_str()) {
                        bp.to_string()
                    } else if !p.manifest.ffi_bindings.binary_path.is_empty() {
                        p.manifest.ffi_bindings.binary_path.clone()
                    } else {
                        p.manifest.native_binary.clone()
                    }
                } else if !p.manifest.ffi_bindings.binary_path.is_empty() {
                    p.manifest.ffi_bindings.binary_path.clone()
                } else {
                    p.manifest.native_binary.clone()
                };
                p.path.join(binary)
            })
    }
}


