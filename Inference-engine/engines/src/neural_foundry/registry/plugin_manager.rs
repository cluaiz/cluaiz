use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use inference_cel::parser::metadata_parser::IntegrationMetadata;

// ─── Plugin Runtime Wrapper ───────────────────────────────────────────────────

pub struct Plugin {
    pub manifest: IntegrationMetadata,
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
        let _path = crate::neural_foundry::registry::download_manager::DownloadManager::download_and_extract(&hub_url, "plugin", plugin_name).await?;
        
        tracing::info!("⬇️ [PluginManager] Plugin files downloaded for {} from {}", plugin_name, hub_url);

        // 3. 🛡️ Run CEL Safety Checker (4-Step Audit) BEFORE registration
        let manifest_plugin_path = _path.join("manifest-plugin.yaml");
        let manifest_json_path = _path.join("manifest.json");
        let active_manifest_path = if manifest_plugin_path.exists() { &manifest_plugin_path } else { &manifest_json_path };
        
        if active_manifest_path.exists() {
            let content = std::fs::read_to_string(active_manifest_path)?;
            let manifest_val: serde_json::Value = if active_manifest_path.extension().unwrap_or_default() == "yaml" {
                serde_yaml::from_str(&content).map_err(|e| anyhow::anyhow!("YAML Parse error: {}", e))?
            } else {
                serde_json::from_str(&content)?
            };
            
            let mut binary_name = manifest_val.get("execution").and_then(|e| e.get("binary_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            
            if binary_name.is_empty() {
                // Auto-discovery
                if let Ok(entries) = std::fs::read_dir(&_path) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().map_or(false, |ext| ext == "wasm" || ext == "dll" || ext == "so" || ext == "dylib") {
                            binary_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            break;
                        }
                    }
                }
            }

            let binary_path = _path.join(&binary_name);
            
            inference_cel::execution::safety_checker::SafetyChecker::audit_plugin(active_manifest_path, &binary_path, &manifest_val)
                .map_err(|e| anyhow::anyhow!("Safety Audit Failed: {}", e))?;
        } else {
            return Err(anyhow::anyhow!("Safety Audit Failed: Invalid or missing manifest."));
        }

        // 4. Write to registry.yaml
        use crate::neural_foundry::registry::registry_index::{MasterRegistry, RegistryEntry, LoadStrategy};
        let domain = format!("plugin/{}", plugin_name);
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
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("plugin");
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
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("plugin");
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
    /// Priority: manifest-plugin.yaml → manifest.yaml → manifest.json (backwards compat)
    fn load_manifest(dir: &PathBuf) -> Option<IntegrationMetadata> {
        let bin_path = dir.join("manifest-plugin.bin");
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(m) = bincode::deserialize::<IntegrationMetadata>(&bytes) {
                    return Some(m);
                }
            }
        }

        let yaml_path = dir.join("manifest-plugin.yaml");
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
                    cluaiz_shared::dev_info!("🔌 [PluginManager] Found Plugin Muscle: {} at {:?}", manifest.name, path);
                    self.active_plugins.push(Plugin { manifest, path });
                }
            }
        }
        Ok(())
    }

    pub fn get_plugin_binary_path(&self, plugin_name: &str) -> Option<PathBuf> {
        self.active_plugins.iter()
            .find(|p| p.manifest.name == plugin_name)
            .map(|p| {
                let mut binary_name = p.manifest.execution.as_ref().and_then(|e| e.binary_path.clone()).unwrap_or_default();
                if binary_name.is_empty() {
                    // Auto-discovery fallback
                    if let Ok(entries) = std::fs::read_dir(&p.path) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().map_or(false, |ext| ext == "wasm" || ext == "dll" || ext == "so" || ext == "dylib") {
                                binary_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                break;
                            }
                        }
                    }
                }
                p.path.join(binary_name)
            })
    }
}


