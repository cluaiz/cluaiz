use std::path::PathBuf;
use anyhow::{Result, anyhow};
use inference_cel::execution::{native_sandbox::NativeExecutor, wasm_sandbox::WasmExecutor};
use inference_cel::ffi::cxp_ffi::{ExtensionPayload, Transpiler};
use crate::neural_foundry::registry::registry_index::MasterRegistry;
use crate::neural_foundry::registry::plugin_manager::PluginManifest;
use inference_cel::parser::metadata_parser::EngineRules as CelEngineRules;
use cluaize_shared::environment::EnvironmentManager;

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

        let envelope = if let Some(exec) = &manifest.execution {
            exec.get("envelope").and_then(|v| v.as_str()).unwrap_or("WASM").to_string()
        } else {
            "NATIVE".to_string() // Fallback for legacy plugins
        };

        let binary_name = if let Some(exec) = &manifest.execution {
            if let Some(bp) = exec.get("binary_path").and_then(|v| v.as_str()) {
                bp.to_string()
            } else if !manifest.ffi_bindings.binary_path.is_empty() {
                manifest.ffi_bindings.binary_path.clone()
            } else {
                manifest.native_binary.clone()
            }
        } else if !manifest.ffi_bindings.binary_path.is_empty() {
            manifest.ffi_bindings.binary_path.clone()
        } else {
            manifest.native_binary.clone()
        };

        if binary_name.is_empty() {
            return Err(anyhow!("Plugin '{}' manifest missing binary_path", plugin_name));
        }

        let binary_path = domain_path.join(&binary_name);
        if !binary_path.exists() {
            return Err(anyhow!("Plugin binary missing at: {}", binary_path.display()));
        }

        let binary_path_str = binary_path.to_string_lossy().to_string();

        let cel_rules = CelEngineRules {
            sandbox_type: envelope.clone(),
            max_memory_mb: manifest.engine_rules.max_memory_mb.map(|v| v as u64),
            allow_network: Some(manifest.engine_rules.allow_network),
            allow_file_system: Some(manifest.engine_rules.allow_file_system),
            allow_subprocess: Some(manifest.engine_rules.allow_subprocess),
            allow_env_vars: Some(manifest.engine_rules.allow_env_vars),
            fuel_limit: manifest.engine_rules.fuel_limit,
            timeout_ms: manifest.engine_rules.timeout_ms,
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
}
