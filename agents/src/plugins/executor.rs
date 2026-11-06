//! Plugin executor (`agents/src/plugins/executor.rs`).
//!
//! A `.wasm` plugin runs inside the Engine, closed in ([`super::wasm`]). Any other plugin runs as
//! a plain child process in its own folder, with the same rights as the Engine, and its stdout
//! is the answer.

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
        if binary.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wasm")) {
            let max_memory_mb = manifest.permissions.as_ref().and_then(|p| p.max_memory_mb).map(u64::from);
            return super::wasm::run_file(&binary, payload, max_memory_mb);
        }
        
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
