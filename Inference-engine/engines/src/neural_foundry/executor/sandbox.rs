use std::path::PathBuf;
use anyhow::{Result, anyhow};
use inference_cel::execution::{native_sandbox::NativeExecutor, wasm_sandbox::WasmExecutor};
use inference_cel::ffi::cxp_ffi::{ExtensionPayload, Transpiler};
use crate::neural_foundry::registry::registry_index::MasterRegistry;
use inference_cel::parser::metadata_parser::IntegrationMetadata;
use inference_cel::parser::metadata_parser::EngineRules as CelEngineRules;
use cluaiz_shared::environment::EnvironmentManager;

/// A unified executor that routes payloads to either Native (C-FFI) or WASM sandboxes
/// based on the plugin's manifest envelope, strictly following Master Registry state.
pub struct UnifiedExecutor {
    native_exec: NativeExecutor,
    wasm_exec: WasmExecutor,
}

impl Default for UnifiedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl UnifiedExecutor {
    pub fn new() -> Self {
        Self {
            native_exec: NativeExecutor::new(),
            wasm_exec: WasmExecutor::new(),
        }
    }

    /// Executes a plugin by name. Automatically resolves domain, parses manifest,
    /// checks security envelope, and dispatches to the correct sandbox.
    pub fn execute(&self, plugin_name: &str, payload: &ExtensionPayload) -> Result<Vec<u8>> {
        let registry = MasterRegistry::load()
            .map_err(|e| anyhow!("Failed to load MasterRegistry: {}", e))?;
        
        let entry = registry.plugins.get(plugin_name)
            .ok_or_else(|| anyhow!("Plugin '{}' not found in registry", plugin_name))?;

        if !entry.enabled {
            return Err(anyhow!("Plugin '{}' is disabled in registry", plugin_name));
        }

        // Resolve absolute domain path
        let env = EnvironmentManager::current();
        let domain_path = env.global_dir.join(&entry.domain);
        
        if !domain_path.exists() {
            return Err(anyhow!("Plugin domain path missing: {}", domain_path.display()));
        }

        // Parse Manifest
        let manifest = Self::load_manifest(&domain_path)
            .ok_or_else(|| anyhow!("Failed to load manifest for plugin '{}'", plugin_name))?;

        let envelope = manifest.execution.as_ref().and_then(|e| e.envelope.clone()).unwrap_or_else(|| "WASM".to_string());

        let mut binary_name = manifest.execution.as_ref().and_then(|e| e.binary_path.clone()).unwrap_or_default();
        if binary_name.is_empty() {
            // Auto-discovery fallback for plugins/extensions
            if let Ok(entries) = std::fs::read_dir(&domain_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "wasm" || ext == "dll" || ext == "so" || ext == "dylib") {
                        binary_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        break;
                    }
                }
            }
        }

        if binary_name.is_empty() {
            return Err(anyhow!("Plugin '{}' manifest missing binary_path and no binary found in domain", plugin_name));
        }

        let binary_path = domain_path.join(&binary_name);
        if !binary_path.exists() {
            return Err(anyhow!("Plugin binary missing at: {}", binary_path.display()));
        }

        let binary_path_str = binary_path.to_string_lossy().to_string();

        let cel_rules = CelEngineRules {
            sandbox_type: envelope.clone(),
            max_memory_mb: manifest.permissions.as_ref().and_then(|p| p.max_memory_mb),
            allow_network: manifest.permissions.as_ref().and_then(|p| p.network_access),
            allow_file_system: manifest.permissions.as_ref().and_then(|p| p.file_system.clone()).map(|fs| fs != "none"),
            allow_subprocess: Some(false), // By default disabled in new schema
            allow_env_vars: Some(false),   // By default disabled in new schema
            fuel_limit: Some(500_000),     // Fallback default
            timeout_ms: manifest.permissions.as_ref().and_then(|p| p.max_cpu_time_ms),
        };

        match envelope.as_str() {
            "NATIVE" => {
                tracing::info!("🔌 [UnifiedExecutor] Routing {} to Native C-FFI Sandbox", plugin_name);
                self.native_exec.execute_with_rules(
                    &binary_path_str,
                    payload,
                    &cel_rules
                ).map_err(|e| anyhow!("Native Execution Error: {}", e))
            }
            "WASM" => {
                tracing::info!("🕸️ [UnifiedExecutor] Routing {} to WASM Sandbox", plugin_name);
                
                // Read WASM bytes to cache if not cached
                let wasm_bytes = std::fs::read(&binary_path)
                    .map_err(|e| anyhow!("Failed to read WASM binary: {}", e))?;
                
                // Preload compiles and caches it if it hasn't been already
                let _ = self.wasm_exec.preload_cache(plugin_name, &wasm_bytes);

                // Get raw bytes from payload since WASM executor expects byte slices
                let payload_bytes = unsafe { payload.as_bytes() };

                self.wasm_exec.execute_with_rules(
                    plugin_name,
                    payload_bytes,
                    &cel_rules
                ).map_err(|e| anyhow!("WASM Execution Error: {}", e))
            }
            other => Err(anyhow!("Unsupported execution envelope: {}", other))
        }
    }

    fn load_manifest(dir: &PathBuf) -> Option<IntegrationMetadata> {
        // Check for .bin files first
        let bin_candidates = ["manifest-plugin.bin", "manifest-extension.bin", "manifest-mcp.bin"];
        for candidate in bin_candidates {
            let bin_path = dir.join(candidate);
            if bin_path.exists() {
                if let Ok(bytes) = std::fs::read(&bin_path) {
                    if let Ok(m) = bincode::deserialize::<IntegrationMetadata>(&bytes) {
                        return Some(m);
                    }
                }
            }
        }

        // Fallback to yaml files and auto-compile
        let candidates = ["manifest-plugin.yaml", "manifest-extension.yaml", "manifest-mcp.yaml"];
        for candidate in candidates {
            let yaml_path = dir.join(candidate);
            if yaml_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                    if let Ok(m) = serde_yaml::from_str::<IntegrationMetadata>(&content) {
                        let bin_name = candidate.replace(".yaml", ".bin");
                        if let Ok(bin_data) = bincode::serialize(&m) {
                            let _ = std::fs::write(dir.join(bin_name), bin_data);
                        }
                        return Some(m);
                    }
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
}
