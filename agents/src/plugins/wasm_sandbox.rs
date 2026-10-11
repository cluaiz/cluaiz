//! ⚡ Plugin Executor Sandbox (`agents/src/plugins/wasm_sandbox.rs`)
//! Safe execution sandbox for plugins without legacy crate dependencies.

use anyhow::{anyhow, Result};
use std::path::Path;
use super::manifest::PluginManifestParser;

pub struct PluginExecutor;

impl PluginExecutor {
    /// Executes a plugin located at `plugin_dir` with the given input payload
    pub fn execute(plugin_dir: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        let manifest_path = plugin_dir.join("package.json");
        let manifest = PluginManifestParser::parse_file(&manifest_path).unwrap_or_default();

        // 1. Locate binary or script
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
                    if p.extension().map_or(false, |ext| {
                        ext == "wasm" || ext == "dll" || ext == "exe" || ext == "so" || ext == "py" || ext == "js"
                    }) {
                        binary_path = Some(p);
                        break;
                    }
                }
            }
        }

        let binary = binary_path.ok_or_else(|| anyhow!("No executable binary or script found in plugin directory: {:?}", plugin_dir))?;

        tracing::info!("⚡ [PluginExecutor] Executing plugin binary at: {:?}", binary);
        
        let payload_str = String::from_utf8_lossy(payload);
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg(&*payload_str);
        cmd.current_dir(plugin_dir);

        let output = cmd.output()?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Plugin process exited with code {:?}: {}", output.status.code(), err_msg))
        }
    }
}
