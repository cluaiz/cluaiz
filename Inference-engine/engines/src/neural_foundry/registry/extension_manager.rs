use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use inference_cel::parser::metadata_parser::IntegrationMetadata;

// ─── Extension Runtime Wrapper ────────────────────────────────────────────────

pub struct Extension {
    pub manifest: IntegrationMetadata,
    pub path: PathBuf,
}

pub struct ExtensionManager {
    pub active_extensions: Vec<Extension>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            active_extensions: Vec::new(),
        }
    }

    pub async fn install_extension(extension_name: &str) -> anyhow::Result<()> {
        // 1. TODO: Download actual files from hub
        tracing::info!("⬇️ [ExtensionManager] Extension files downloaded for {}", extension_name);

        // 2. Write to registry.yaml
        use crate::neural_foundry::registry::registry_index::{MasterRegistry, RegistryEntry, LoadStrategy};
        let domain = format!("extension/{}", extension_name);
        let entry = RegistryEntry {
            id: format!("ext_{}_{}", extension_name, chrono::Utc::now().timestamp()),
            domain,
            load_strategy: LoadStrategy::Lazy,
            activation_events: vec![
                format!("on_command:use extension::{}", extension_name),
            ],
            enabled: true,
            binary_hash: None,
            semantic_index: None,
        };

        let mut registry = MasterRegistry::load()?;
        registry.register_component("extension", extension_name, entry)?;
        
        Ok(())
    }

    pub async fn remove_extension(extension_name: &str) -> anyhow::Result<()> {
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("extension");
        // We iterate subdirectories since extensions can be nested in domain folders like core/brain
        let mut found_path = None;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    for sub_entry in std::fs::read_dir(&path)? {
                        let sub_entry = sub_entry?;
                        let sub_path = sub_entry.path();
                        if sub_path.file_name().unwrap_or_default() == extension_name {
                            found_path = Some(sub_path);
                            break;
                        }
                    }
                }
            }
        }
        
        if let Some(path) = found_path {
            // Remove files
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_dir_all(&path);
            }).await?;
            
            // Remove from registry.yaml
            use crate::neural_foundry::registry::registry_index::MasterRegistry;
            if let Ok(mut registry) = MasterRegistry::load() {
                let _ = registry.deregister_component("extension", extension_name);
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Extension '{}' not found on disk", extension_name))
        }
    }

    pub async fn clear_extension_cache(extension_name: Option<&str>) -> anyhow::Result<usize> {
        let base_path = cluaiz_shared::environment::EnvironmentManager::current().global_dir.join("extension");
        let mut wiped = 0;
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    for sub_entry in std::fs::read_dir(&path)? {
                        let sub_entry = sub_entry?;
                        let sub_path = sub_entry.path();
                        if let Some(ext_name) = extension_name {
                            if sub_path.file_name().unwrap_or_default() != ext_name { continue; }
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

    /// Load manifest from a component directory.
    /// Priority: manifest.yaml (new standard) → manifest.json (backwards compat)
    fn load_manifest(dir: &PathBuf) -> Option<IntegrationMetadata> {
        let bin_path = dir.join("manifest-extension.bin");
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(m) = bincode::deserialize::<IntegrationMetadata>(&bytes) {
                    return Some(m);
                }
            }
        }

        // 1. Prefer manifest-extension.yaml (new Two-Tier Architecture standard)
        let yaml_path = dir.join("manifest-extension.yaml");
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                if let Ok(m) = serde_yaml::from_str::<IntegrationMetadata>(&content) {
                    if let Ok(bin_data) = bincode::serialize(&m) {
                        let _ = std::fs::write(&bin_path, bin_data);
                    }
                    return Some(m);
                } else {
                    tracing::warn!("⚠️ [ExtensionManager] Failed to parse manifest-extension.yaml in {:?}", dir);
                }
            }
        }
        // 2. Fallback to manifest.json (backwards compatibility)
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

    /// Dynamically load extensions from a given domain path.
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
                    cluaiz_shared::dev_info!("🧩 [ExtensionManager] Found Extension: {} at {:?}", manifest.name, path);
                    self.active_extensions.push(Extension { manifest, path });
                }
            }
        }
        Ok(())
    }

    pub fn execute(&self, extension_name: &str, payload_json: &str) -> Result<String> {
        let ext = self.active_extensions.iter().find(|e| e.manifest.name == extension_name)
            .ok_or_else(|| anyhow::anyhow!("Extension '{}' not found", extension_name))?;

        let mut binary_name = ext.manifest.execution.as_ref().and_then(|e| e.binary_path.clone()).unwrap_or_default();
        if binary_name.is_empty() {
            // Auto-discovery fallback for Extension binaries if omitted in manifest
            if let Ok(entries) = std::fs::read_dir(&ext.path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "dll" || ext == "so" || ext == "dylib") {
                        binary_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        break;
                    }
                }
            }
        }

        let lib_path = ext.path.join(&binary_name);
        if !lib_path.exists() || binary_name.is_empty() {
            return Err(anyhow::anyhow!("Extension library not found at {:?}", lib_path));
        }

        let cache_dir = ext.path.join(".cache");
        if !cache_dir.exists() {
            let _ = std::fs::create_dir_all(&cache_dir);
        }

        tracing::info!("🚀 [ExtensionManager] Dispatching payload to {} ({:?})", extension_name, lib_path);

        unsafe {
            let lib = libloading::Library::new(&lib_path)?;
            let execute_cel: libloading::Symbol<unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_char> = lib.get(b"execute_cel\0")?;
            let free_cel_response: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_char)> = lib.get(b"free_cel_response\0")?;

            let c_payload = std::ffi::CString::new(payload_json)?;
            let res_ptr = execute_cel(c_payload.as_ptr());
            
            if res_ptr.is_null() {
                return Err(anyhow::anyhow!("Extension returned null pointer"));
            }

            let c_str = std::ffi::CStr::from_ptr(res_ptr);
            let response = c_str.to_string_lossy().into_owned();

            free_cel_response(res_ptr);

            Ok(response)
        }
    }
}
