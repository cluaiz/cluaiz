use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

// ─── Sandbox Type ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SandboxType {
    /// Runs inside a Wasmtime sandbox — maximum security, memory-safe
    Wasm,
    /// Loads .dll/.so directly via libloading — maximum performance, requires trust
    Native,
    /// Spawns as a child OS process — high isolation, lower performance
    Process,
}

impl Default for SandboxType {
    fn default() -> Self { SandboxType::Wasm }
}

// ─── AI Interface (How AI model knows when and how to call this component) ────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiInterface {
    /// Semantic keywords that trigger this extension (for model routing)
    #[serde(default)]
    pub keywords: Vec<String>,

    /// CEL call syntax (shown to AI during skill execution)
    pub cel_syntax: Option<String>,

    /// What the CEL call returns (JSON schema description)
    pub cel_returns: Option<String>,

    /// Human-readable usage example
    pub usage_example: Option<String>,
}

// ─── Engine Rules (Hardware limits and permissions for this component) ────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineRules {
    /// Execution sandbox: WASM (safe), NATIVE (fast), PROCESS (isolated)
    #[serde(default)]
    pub sandbox_type: SandboxType,

    /// Hard RAM cap in MB. Engine will OOM-kill if exceeded. None = no limit.
    pub max_memory_mb: Option<u32>,

    /// WASM instruction fuel limit (prevents infinite loops). None = no limit.
    pub fuel_limit: Option<u64>,

    /// Maximum execution time per CEL call in milliseconds. None = no timeout.
    pub timeout_ms: Option<u64>,

    /// Can this component make outbound HTTP/network requests?
    #[serde(default)]
    pub allow_network: bool,

    /// Can this component read/write to the local filesystem?
    #[serde(default)]
    pub allow_file_system: bool,

    /// Can this component read OS environment variables?
    #[serde(default)]
    pub allow_env_vars: bool,

    /// Can this component spawn child processes? (Only used with PROCESS sandbox)
    #[serde(default)]
    pub allow_subprocess: bool,
}

impl Default for EngineRules {
    fn default() -> Self {
        Self {
            sandbox_type: SandboxType::Wasm,
            max_memory_mb: Some(128),
            fuel_limit: Some(500_000),
            timeout_ms: Some(30_000),
            allow_network: false,
            allow_file_system: false,
            allow_env_vars: false,
            allow_subprocess: false,
        }
    }
}

// ─── FFI Bindings (How engine finds and calls the compiled binary) ────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FfiBindings {
    /// Path to binary, relative to this manifest's folder
    /// e.g., "native/cluaizd_engine.dll" or "core.wasm"
    pub binary_path: String,

    /// Universal CEL entry point function name (always "execute_cel")
    #[serde(default = "default_entry_point")]
    pub entry_point: String,
}

fn default_entry_point() -> String { "execute_cel".to_string() }

impl Default for FfiBindings {
    fn default() -> Self {
        Self {
            binary_path: String::new(),
            entry_point: "execute_cel".to_string(),
        }
    }
}

// ─── Storage Config ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct StorageConfig {
    /// Relative domain under ~/.cluaize/ e.g., "core/cluaize-db"
    #[serde(default)]
    pub domain: String,

    /// Cache directory name inside the component folder (default: ".cache")
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,

    /// Persistent data directory (None = no persistent data)
    pub data_dir: Option<String>,
}

fn default_cache_dir() -> String { ".cache".to_string() }

// ─── Full Extension Manifest ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Storage domain — kept for backwards compat, mirrors storage.domain
    #[serde(default)]
    pub storage_domain: String,
    /// Backwards-compat: old "entrypoint" field maps to ffi_bindings.binary_path
    #[serde(default)]
    pub entrypoint: String,

    /// AI interface: how the AI model knows when and how to call this
    #[serde(default)]
    pub ai_interface: Option<AiInterface>,

    /// Engine rules: hardware limits and security permissions
    #[serde(default)]
    pub engine_rules: EngineRules,

    /// FFI bindings: binary path and entry point
    #[serde(default)]
    pub ffi_bindings: FfiBindings,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
}

// ─── Extension Runtime Wrapper ────────────────────────────────────────────────

pub struct Extension {
    pub manifest: ExtensionManifest,
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
        let domain = format!("core/{}", extension_name);
        let entry = RegistryEntry {
            id: format!("ext_{}_{}", extension_name, chrono::Utc::now().timestamp()),
            domain,
            load_strategy: LoadStrategy::Lazy,
            activation_events: vec![
                format!("on_command:use extension::{}", extension_name),
            ],
            enabled: true,
            binary_hash: None,
        };

        let mut registry = MasterRegistry::load()?;
        registry.register_component("extension", extension_name, entry)?;
        
        Ok(())
    }

    pub async fn remove_extension(extension_name: &str) -> anyhow::Result<()> {
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("core");
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
        let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("core");
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
    fn load_manifest(dir: &PathBuf) -> Option<ExtensionManifest> {
        // 1. Prefer manifest.yaml (new Two-Tier Architecture standard)
        let yaml_path = dir.join("manifest.yaml");
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                if let Ok(m) = serde_yaml::from_str::<ExtensionManifest>(&content) {
                    return Some(m);
                } else {
                    tracing::warn!("⚠️ [ExtensionManager] Failed to parse manifest.yaml in {:?}", dir);
                }
            }
        }
        // 2. Fallback to manifest.json (backwards compatibility)
        let json_path = dir.join("manifest.json");
        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(m) = serde_json::from_str::<ExtensionManifest>(&content) {
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
                    cluaize_shared::dev_info!("🧩 [ExtensionManager] Found Extension: {} at {:?}", manifest.name, path);
                    self.active_extensions.push(Extension { manifest, path });
                }
            }
        }
        Ok(())
    }

    /// Execute a CEL payload via the native DLL/SO using C FFI
    pub fn execute(&self, extension_name: &str, payload_json: &str) -> Result<String> {
        let ext = self.active_extensions.iter().find(|e| e.manifest.name == extension_name)
            .ok_or_else(|| anyhow::anyhow!("Extension '{}' not found", extension_name))?;

        let lib_path = ext.path.join(&ext.manifest.entrypoint);
        if !lib_path.exists() {
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
