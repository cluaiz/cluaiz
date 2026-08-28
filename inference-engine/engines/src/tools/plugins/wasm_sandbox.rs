use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::LazyLock;
use super::manifest::{PluginManifest, PluginManifestParser};
use inference_cel::{WasmExecutor, EngineRules};
use inference_cel::ffi::cxp_ffi::{CxpPayload, PayloadType};
use inference_cel::execution::native_sandbox::NativeExecutor;

static WASM_ENGINE: LazyLock<WasmExecutor> = LazyLock::new(WasmExecutor::new);

/// Safe execution sandbox for WASM and Native plugin binaries
pub struct PluginExecutor;

impl PluginExecutor {
    /// Executes a plugin located at `plugin_dir` with the given input payload
    pub fn execute(plugin_dir: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        let manifest_path = plugin_dir.join("manifest-plugin.yaml");
        let manifest = PluginManifestParser::parse_file(&manifest_path)
            .unwrap_or_default();

        let envelope = manifest.execution.as_ref()
            .map(|e| e.envelope.as_str())
            .unwrap_or("WASM");

        // Find binary
        let mut binary_path = None;
        if let Some(ref exec) = manifest.execution {
            if let Some(ref b) = exec.binary_path {
                let p = plugin_dir.join(b);
                if p.exists() {
                    binary_path = Some(p);
                }
            }
        }

        if binary_path.is_none() {
            if let Ok(entries) = std::fs::read_dir(plugin_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().map_or(false, |ext| ext == "wasm" || ext == "dll" || ext == "so" || ext == "dylib") {
                        binary_path = Some(p);
                        break;
                    }
                }
            }
        }

        let binary = binary_path.ok_or_else(|| anyhow!("No binary found in plugin directory: {:?}", plugin_dir))?;

        if envelope == "WASM" || binary.extension().map_or(false, |e| e == "wasm") {
            Self::execute_wasm(&manifest, &binary, payload)
        } else {
            Self::execute_native(&manifest, &binary, payload)
        }
    }

    fn execute_wasm(manifest: &PluginManifest, wasm_path: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        tracing::info!("⚡ [PluginExecutor] Executing WASM binary in Wasmtime sandbox: {:?}", wasm_path);
        let wasm_bytes = std::fs::read(wasm_path)?;
        if wasm_bytes.is_empty() {
            return Err(anyhow!("WASM binary is empty"));
        }

        let plugin_id = if !manifest.name.is_empty() {
            manifest.name.as_str()
        } else {
            wasm_path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin")
        };

        // Preload to RAM cache if not yet cached
        let _ = WASM_ENGINE.preload_cache(plugin_id, &wasm_bytes);

        // Build EngineRules from manifest
        let rules = EngineRules {
            sandbox_type: "WASM".to_string(),
            max_memory_mb: manifest.permissions.as_ref().and_then(|p| p.max_memory_mb),
            fuel_limit: Some(1_000_000),
            timeout_ms: manifest.permissions.as_ref().and_then(|p| p.max_cpu_time_ms),
            allow_network: manifest.permissions.as_ref().map(|p| p.network_access),
            allow_file_system: manifest.permissions.as_ref().map(|p| p.file_system != "none"),
            allow_env_vars: Some(false),
            allow_subprocess: Some(false),
        };

        WASM_ENGINE.execute_with_rules(plugin_id, payload, &rules)
            .map_err(|e| anyhow!("WASM Execution error: {}", e))
    }

    fn execute_native(manifest: &PluginManifest, native_path: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        tracing::info!("⚡ [PluginExecutor] Executing Native C-ABI binary: {:?}", native_path);
        if !native_path.exists() {
            return Err(anyhow!("Native binary not found: {:?}", native_path));
        }

        let plugin_id = if !manifest.name.is_empty() {
            manifest.name.as_str()
        } else {
            native_path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin")
        };

        let cxp_payload = CxpPayload::new(PayloadType::Json, payload);
        let native_executor = NativeExecutor::new();
        let rules = EngineRules {
            sandbox_type: "NATIVE".to_string(),
            max_memory_mb: manifest.permissions.as_ref().and_then(|p| p.max_memory_mb),
            fuel_limit: None,
            timeout_ms: manifest.permissions.as_ref().and_then(|p| p.max_cpu_time_ms),
            allow_network: manifest.permissions.as_ref().map(|p| p.network_access),
            allow_file_system: manifest.permissions.as_ref().map(|p| p.file_system != "none"),
            allow_env_vars: Some(false),
            allow_subprocess: Some(false),
        };

        native_executor.execute_with_rules(plugin_id, &cxp_payload, &rules)
            .map_err(|e| anyhow!("Native dynamic execution error: {}", e))
    }
}
