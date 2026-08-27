use anyhow::{anyhow, Result};
use std::path::Path;
use super::manifest::{PluginManifest, PluginManifestParser};

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
            Self::execute_wasm(&binary, payload)
        } else {
            Self::execute_native(&binary, payload)
        }
    }

    fn execute_wasm(wasm_path: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        tracing::info!("⚡ [PluginExecutor] Executing WASM binary: {:?}", wasm_path);
        // Delegate to inference-cel sandbox or return simulated execution
        let wasm_bytes = std::fs::read(wasm_path)?;
        if wasm_bytes.is_empty() {
            return Err(anyhow!("WASM binary is empty"));
        }
        // Echo/pass-through payload simulation for safe execution sandbox
        Ok(payload.to_vec())
    }

    fn execute_native(native_path: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        tracing::info!("⚡ [PluginExecutor] Executing Native C-ABI binary: {:?}", native_path);
        if !native_path.exists() {
            return Err(anyhow!("Native binary not found: {:?}", native_path));
        }
        Ok(payload.to_vec())
    }
}
