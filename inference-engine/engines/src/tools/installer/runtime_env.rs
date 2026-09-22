use std::path::{Path, PathBuf};
use std::process::Stdio;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use engine_core::environment::EnvironmentManager;

/// The detected runtime environment requirement for a tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeType {
    Python,
    Node,
    Native,
    PureSkill,
}

/// Status of the tool's runtime environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStatus {
    pub ready: bool,
    pub runtime: RuntimeType,
    pub executable_path: Option<PathBuf>,
    pub message: String,
}

/// Manages isolated runtime environments for tools, plugins, and MCP servers.
/// Ensures zero pollution of the host system by isolating Python venvs and local node_modules.
pub struct RuntimeEnvironmentManager;

impl RuntimeEnvironmentManager {
    /// Returns the global virtual environments directory (~/.cluaiz/venvs/)
    pub fn venvs_dir() -> PathBuf {
        let env = EnvironmentManager::current();
        env.global_dir.join("venvs")
    }

    /// Returns the specific virtual environment path for a tool ID (~/.cluaiz/venvs/<tool_id>/)
    pub fn tool_venv_dir(tool_id: &str) -> PathBuf {
        Self::venvs_dir().join(tool_id)
    }

    /// Detects the required runtime environment from the tool's directory contents
    pub fn detect_runtime(tool_dir: &Path) -> RuntimeType {
        // 1. Check for native binaries (.dll, .so, .dylib, .wasm, .exe)
        if let Ok(entries) = std::fs::read_dir(tool_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if matches!(ext_str.as_str(), "dll" | "so" | "dylib" | "wasm" | "exe") {
                        return RuntimeType::Native;
                    }
                }
            }
        }

        // 2. Check for Python requirements
        if tool_dir.join("requirements.txt").exists() 
            || tool_dir.join("pyproject.toml").exists() 
            || tool_dir.join("setup.py").exists() {
            return RuntimeType::Python;
        }

        // 3. Check for Node / MCP package.json
        if tool_dir.join("package.json").exists() {
            return RuntimeType::Node;
        }

        // 4. Default to Pure Markdown / Declarative Skill
        RuntimeType::PureSkill
    }

    /// Checks whether the tool's runtime environment is already provisioned and ready
    pub fn is_environment_ready(tool_dir: &Path, tool_id: &str) -> bool {
        match Self::detect_runtime(tool_dir) {
            RuntimeType::PureSkill => true,
            RuntimeType::Native => {
                // Ensure at least one binary exists
                if let Ok(entries) = std::fs::read_dir(tool_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(ext) = p.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if matches!(ext_str.as_str(), "dll" | "so" | "dylib" | "wasm" | "exe") {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            RuntimeType::Node => {
                // If package.json has dependencies, check node_modules
                let pkg_json = tool_dir.join("package.json");
                if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        let has_deps = val.get("dependencies").and_then(|d| d.as_object()).map_or(false, |o| !o.is_empty());
                        if has_deps {
                            return tool_dir.join("node_modules").exists();
                        }
                    }
                }
                true
            }
            RuntimeType::Python => {
                let venv = Self::tool_venv_dir(tool_id);
                #[cfg(target_os = "windows")]
                let py = venv.join("Scripts").join("python.exe");
                #[cfg(not(target_os = "windows"))]
                let py = venv.join("bin").join("python");

                py.exists()
            }
        }
    }

    /// Asynchronously provisions the isolated environment for the tool.
    /// Runs silently in the background, isolating packages to avoid host system pollution.
    pub async fn provision_environment(tool_dir: &Path, tool_id: &str) -> Result<EnvironmentStatus> {
        let runtime = Self::detect_runtime(tool_dir);

        match runtime {
            RuntimeType::PureSkill => {
                Ok(EnvironmentStatus {
                    ready: true,
                    runtime,
                    executable_path: None,
                    message: "Pure declarative skill, zero package dependencies required.".to_string(),
                })
            }
            RuntimeType::Native => {
                let mut binary_path = None;
                if let Ok(entries) = std::fs::read_dir(tool_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(ext) = p.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if matches!(ext_str.as_str(), "dll" | "so" | "dylib" | "wasm" | "exe") {
                                binary_path = Some(p);
                                break;
                            }
                        }
                    }
                }

                if let Some(bin) = binary_path {
                    Ok(EnvironmentStatus {
                        ready: true,
                        runtime,
                        executable_path: Some(bin.clone()),
                        message: format!("Native binary ready: {:?}", bin.file_name().unwrap_or_default()),
                    })
                } else {
                    Err(anyhow!("No native compiled binary (.dll/.so/.wasm/.exe) found in {:?}", tool_dir))
                }
            }
            RuntimeType::Node => {
                let pkg_json = tool_dir.join("package.json");
                let has_deps = if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                    serde_json::from_str::<serde_json::Value>(&content)
                        .ok()
                        .and_then(|v| v.get("dependencies").and_then(|d| d.as_object()).map(|o| !o.is_empty()))
                        .unwrap_or(false)
                } else {
                    false
                };

                if has_deps && !tool_dir.join("node_modules").exists() {
                    tracing::info!("📦 [RuntimeEnv] Installing local npm packages for '{}' in {:?}", tool_id, tool_dir);

                    #[cfg(target_os = "windows")]
                    let npm_cmd = "npm.cmd";
                    #[cfg(not(target_os = "windows"))]
                    let npm_cmd = "npm";

                    let status = Command::new(npm_cmd)
                        .args(["install", "--no-audit", "--no-fund", "--prefer-offline"])
                        .current_dir(tool_dir)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .status()
                        .await
                        .map_err(|e| anyhow!("Failed to execute npm install: {}", e))?;

                    if !status.success() {
                        return Err(anyhow!("npm install failed with exit code {:?}", status.code()));
                    }
                }

                Ok(EnvironmentStatus {
                    ready: true,
                    runtime,
                    executable_path: None,
                    message: "Node.js tool environment isolated and ready.".to_string(),
                })
            }
            RuntimeType::Python => {
                let venv_dir = Self::tool_venv_dir(tool_id);
                std::fs::create_dir_all(Self::venvs_dir())?;

                #[cfg(target_os = "windows")]
                let py_exe = venv_dir.join("Scripts").join("python.exe");
                #[cfg(not(target_os = "windows"))]
                let py_exe = venv_dir.join("bin").join("python");

                if !py_exe.exists() {
                    tracing::info!("🐍 [RuntimeEnv] Creating isolated Python venv for '{}' at {:?}", tool_id, venv_dir);

                    // Try 'uv venv' first (market standard fastest), fallback to 'python -m venv'
                    let uv_spawn = Command::new("uv")
                        .args(["venv", venv_dir.to_str().unwrap_or_default()])
                        .status()
                        .await;

                    let created = match uv_spawn {
                        Ok(st) if st.success() => true,
                        _ => {
                            #[cfg(target_os = "windows")]
                            let base_py = "python";
                            #[cfg(not(target_os = "windows"))]
                            let base_py = "python3";

                            Command::new(base_py)
                                .args(["-m", "venv", venv_dir.to_str().unwrap_or_default()])
                                .status()
                                .await
                                .map(|s| s.success())
                                .unwrap_or(false)
                        }
                    };

                    if !created || !py_exe.exists() {
                        return Err(anyhow!("Failed to create isolated virtual environment at {:?}", venv_dir));
                    }
                }

                // Install requirements.txt if present
                let req_txt = tool_dir.join("requirements.txt");
                if req_txt.exists() {
                    tracing::info!("📦 [RuntimeEnv] Installing Python dependencies for '{}' into isolated venv", tool_id);

                    let install_status = Command::new(&py_exe)
                        .args(["-m", "pip", "install", "-q", "-r", req_txt.to_str().unwrap_or_default()])
                        .current_dir(tool_dir)
                        .status()
                        .await
                        .map_err(|e| anyhow!("Failed to run pip install inside venv: {}", e))?;

                    if !install_status.success() {
                        return Err(anyhow!("pip install failed with exit code {:?}", install_status.code()));
                    }
                }

                Ok(EnvironmentStatus {
                    ready: true,
                    runtime,
                    executable_path: Some(py_exe),
                    message: "Isolated Python venv provisioned and ready.".to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pure_skill() {
        let temp = std::env::temp_dir().join("test_skill_detection");
        let _ = std::fs::create_dir_all(&temp);
        let _ = std::fs::write(temp.join("SKILL.md"), "# Skill");

        assert_eq!(RuntimeEnvironmentManager::detect_runtime(&temp), RuntimeType::PureSkill);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_detect_node_runtime() {
        let temp = std::env::temp_dir().join("test_node_detection");
        let _ = std::fs::create_dir_all(&temp);
        let _ = std::fs::write(temp.join("package.json"), "{}");

        assert_eq!(RuntimeEnvironmentManager::detect_runtime(&temp), RuntimeType::Node);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_detect_native_runtime() {
        let temp = std::env::temp_dir().join("test_native_detection");
        let _ = std::fs::create_dir_all(&temp);
        let _ = std::fs::write(temp.join("engine_plugin.dll"), "fake binary");

        assert_eq!(RuntimeEnvironmentManager::detect_runtime(&temp), RuntimeType::Native);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
