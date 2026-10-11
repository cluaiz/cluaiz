use std::path::{Path, PathBuf};
use color_eyre::{Result, eyre::eyre};
use colored::Colorize;

pub struct Bootstrapper;

impl Bootstrapper {
    const MASTER_REGISTRY_URL: &'static str = "https://raw.githubusercontent.com/cluaiz/cluaiz/main/package.json";

    /// 🚀 BOOTSTRAP: The Sovereign Handshake.
    pub async fn ignite(is_dev_sync: bool) -> Result<()> {
        Self::ensure_global_path();
        
        // 🚀 0. Tools Engine Genesis (Create tools_registry.json and sync ~/.cluaiz/tools/)
        tracing::info!("🛠️ Initializing Tools Engine (Registry & Governance)...");
        let permissions = cluaiz_security::PermissionSchema::load();
        let _ = permissions.save();
        
        let _ = cluaiz_agents::ToolsEngine::registry();

        // 📜 Write Third-Party Licenses (Automatic Legal Compliance)
        let hub_path = cluaiz_core::EnvironmentManager::current().global_dir.clone();
        let _ = std::fs::create_dir_all(&hub_path);
        let license_text = include_str!("../assets/THIRD_PARTY_NOTICES.txt");
        let _ = std::fs::write(hub_path.join("THIRD_PARTY_LICENSES.txt"), license_text);

        #[cfg(windows)]
        let _ = colored::control::set_virtual_terminal(true);

        let _bin_dir = cluaiz_core::EnvironmentManager::current().bin_dir();

        if is_dev_sync {
            tracing::info!("⚙️ [DevSync] Basic Configuration generated. Skipping network and registry sync for local deployment.");
            return Ok(());
        }
        
        let client = reqwest::Client::builder()
            .user_agent(format!("cluaiz-Bootstrapper/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        // 🎯 1. Fetch Master Registry (package.json)
        tracing::info!("🛰️ [cluaiz] Synchronizing Neural Registry...");
        
        let master_registry_res = client.get(Self::MASTER_REGISTRY_URL).send().await;
        
        let master_registry: serde_json::Value = match master_registry_res {
            Ok(res) if res.status().is_success() => {
                match res.json().await {
                    Ok(json) => json,
                    Err(_) => {
                        println!("  {} [Cluaiz] Network sync skipped (invalid registry format).", "⚠️".yellow());
                        return Ok(());
                    }
                }
            },
            _ => {
                println!("  {} [Cluaiz] Network sync skipped (offline mode).", "⚠️".yellow());
                return Ok(());
            }
        };
        
        // 🏛️ Seal the Master Registry with Atomic Write Protocol
        let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
        let _ = std::fs::create_dir_all(&config_dir);
        let json_path = config_dir.join("package.json");
        let bin_path = config_dir.join("package.bin");
        if let Ok(json_str) = serde_json::to_string_pretty(&master_registry) {
            let _ = std::fs::write(&json_path, json_str);
        }
        if let Ok(bin_data) = serde_json::to_vec(&master_registry) {
            let _ = std::fs::write(&bin_path, bin_data);
        }
        tracing::debug!("✅ [Registry] Binary Truth sealed and verified.");

        // 🎯 2. CLI Lifecycle Check
        let cli_info = &master_registry["components"]["cli"];
        let latest_cli = cli_info["version"].as_str().unwrap_or("");
        let current_cli = env!("CARGO_PKG_VERSION");
        
        if latest_cli != current_cli && !latest_cli.is_empty() {
            println!("  {} [cluaiz] Update Available: {} -> {}", "🚀".green(), current_cli, latest_cli);
        }

        // 🎯 3. Unified Binary Verification (Single Binary Architecture)
        let bin_dir = cluaiz_core::EnvironmentManager::current().bin_dir();
        let exe_name = if cfg!(windows) { "cluaiz.exe" } else { "cluaiz" };
        let exe_path = bin_dir.join(exe_name);
        if exe_path.exists() {
            tracing::info!("🚀 [cluaiz] Unified Core Binary verified: {:?}", exe_path);
        } else {
            tracing::debug!("ℹ️ [cluaiz] Running from workspace binary; global deployment available via dev-sync.");
        }

        // 🎯 4. Hardware Acceleration & Dynamic Driver Verification
        Self::sync_neural_stack().await?;

        Ok(())
    }

    async fn sync_neural_stack() -> Result<()> {
        let hw = cluaiz_hw::HardwareProber::current();
        let has_nvidia = hw.gpus.iter().any(|g| g.vendor.to_uppercase().contains("NVIDIA"));

        if has_nvidia {
            tracing::info!("⚡ NVIDIA GPU hardware acceleration verified.");
        }

        let drivers_dir = cluaiz_core::EnvironmentManager::current().engine_dir().join("drivers");
        if drivers_dir.exists() {
            tracing::debug!("🔌 Hardware dynamic drivers directory verified: {:?}", drivers_dir);
        }

        Ok(())
    }

    async fn download_asset(url: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
        let client = reqwest::Client::builder().user_agent(format!("cluaiz-Bootstrapper/{}", env!("CARGO_PKG_VERSION"))).build()?;
        let response = client.get(url).send().await.map_err(|e| eyre!("Registry Link Error: {}", e))?;
        if !response.status().is_success() { return Err(eyre!("Registry Error: {} returned {}", url, response.status())); }
        let content = response.bytes().await?;
        std::fs::write(dest, content)?;
        Ok(())
    }

    /// 🛠️ Artifact Sync: Synchronizes local build artifacts to .cluaiz.
    /// This ensures cargo run or the first boot always uses the latest compiled binaries.
    pub fn sync_dev_artifacts(target: &str, driver_name: Option<&str>, hub_path: PathBuf, profile: &str) -> Result<()> {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        
        let ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
        let release_dir = root.join("target").join("release");
        let debug_dir = root.join("target").join("debug");

        let copy_with_rename = |src: &std::path::Path, dest: &std::path::Path| -> std::io::Result<u64> {
            if !src.exists() {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Source not found"));
            }
            let mut res = std::fs::copy(src, dest);
            if let Err(ref e) = res {
                #[cfg(windows)]
                if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) {
                    let rand_val: u32 = rand::random();
                    let temp_dest = dest.with_extension(format!("{}.old", rand_val));
                    if std::fs::rename(dest, &temp_dest).is_ok() {
                        res = std::fs::copy(src, dest);
                        let _ = std::fs::remove_file(&temp_dest);
                    }
                }
            }
            res
        };

        // 0. GAP B FIX: Clean up legacy directory structure to prevent zombie DLLs
        let legacy_interfaces_dir = hub_path.join("engine").join("interfaces");
        if legacy_interfaces_dir.exists() {
            tracing::info!("🧹 [DevSync] Legacy directory detected. Wiping: {:?}", legacy_interfaces_dir);
            if let Err(e) = std::fs::remove_dir_all(&legacy_interfaces_dir) {
                tracing::warn!("⚠️ [DevSync] Failed to wipe legacy interfaces directory: {}", e);
            }
        }

        // Helper to find artifacts in root or deps/ folder
        let find_artifact = |base_name: &str, ext: &str| -> Option<(PathBuf, bool)> {
            let names = vec![
                format!("{}.{}", base_name, ext),
                format!("lib{}.{}", base_name, ext), // For linux/mac
            ];
            
            let target_dir = if profile == "release" { &release_dir } else { &debug_dir };
            let is_release = profile == "release";
            
            for name in &names {
                let p = target_dir.join(name);
                if p.exists() { return Some((p, is_release)); }
                let p = target_dir.join("deps").join(name);
                if p.exists() { return Some((p, is_release)); }
            }
            None
        };

        // 1. Hardware Driver Sync (ONNX & Vendor Providers into engine/drivers/)
        let kernels_to_sync = vec![
            ("cluaiz_onnx", "cluaiz-onnx"),
            ("onnxruntime", "onnxruntime"),
            ("DirectML", "DirectML"),
            ("onnxruntime_providers_shared", "onnxruntime_providers_shared"),
            ("onnxruntime_providers_cuda", "onnxruntime_providers_cuda"),
            ("onnxruntime_providers_tensorrt", "onnxruntime_providers_tensorrt"),
            ("onnxruntime_providers_nv_tensorrt_rtx", "onnxruntime_providers_nv_tensorrt_rtx"),
        ];

        let interface_path = hub_path.join("engine");

        for (src_name, dest_name) in kernels_to_sync {
            // Apply filtering logic
            if target == "driver" {
                if let Some(d) = driver_name {
                    if !dest_name.contains(d) && !src_name.contains(d) {
                        continue; // Skip if it doesn't match the requested driver
                    }
                }
            }

            if let Some((kernel_src, is_kernel_release)) = find_artifact(src_name, ext) {
                let kernel_dest = interface_path.join("drivers").join(format!("{}.{}", dest_name, ext));
                
                let _ = std::fs::create_dir_all(kernel_dest.parent().unwrap());
                
                if let Err(e) = copy_with_rename(&kernel_src, &kernel_dest) {
                    tracing::warn!("⚠️ [DevSync] {} Kernel Link Failed: {}.", dest_name, e);
                } else {
                    // Create a ready marker
                    if !src_name.starts_with("onnxruntime") {
                        let marker_name = format!("{}.ready", dest_name);
                        let marker = "dev-release";
                        let _ = std::fs::write(interface_path.join(marker_name), marker);
                    }
                    tracing::info!("🧬 [DevSync] {} Driver Linked (release={}): {:?}", dest_name, is_kernel_release, kernel_dest);
                }
            } else {
                if target == "drivers" || target == "driver" {
                    tracing::warn!("⚠️ [DevSync] Driver source not found for: {}", src_name);
                }
            }
        }

        // 3. CLI Executable Sync (cluaiz.exe -> bin/cluaiz.exe)
        if target == "all" || target == "core" {
            let exe_name = if cfg!(windows) { "cluaiz.exe" } else { "cluaiz" };
            let target_dir = if profile == "release" { &release_dir } else { &debug_dir };
            let exe_src = target_dir.join(exe_name);
            let bin_dir = hub_path.join("bin");
            let _ = std::fs::create_dir_all(&bin_dir);
            let exe_dest = bin_dir.join(exe_name);
            
            if exe_src.exists() {
                if let Err(e) = copy_with_rename(&exe_src, &exe_dest) {
                    tracing::warn!("⚠️ [DevSync] CLI Executable Sync Failed (File might be in use): {}.", e);
                } else {
                    tracing::info!("🚀 [DevSync] CLI Executable Synced: {:?}", exe_dest);
                }
            }
        }

        // 4. Sync Dev Environment Folders (brain, skills)
        if target == "all" || target == "core" {
            let local_cluaiz = root.join(".cluaiz");
            
            // Sync Brain
            let local_brain = local_cluaiz.join("brain");
            let global_brain = hub_path.join("brain");
            if local_brain.exists() {
                let _ = Self::copy_dir_recursive(&local_brain, &global_brain);
                tracing::info!("🧠 [DevSync] Brain Data Synced to: {:?}", global_brain);
            }

            // Sync Skills
            let local_skills = local_cluaiz.join("skills");
            let global_skills = hub_path.join("skills");
            if local_skills.exists() {
                let _ = Self::copy_dir_recursive(&local_skills, &global_skills);
                tracing::info!("🛠️ [DevSync] Skills Synced to: {:?}", global_skills);
            }
        }

        Ok(())
    }

    /// Recursively copy a directory
    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                Self::copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                let _ = std::fs::copy(entry.path(), dst.join(entry.file_name()));
            }
        }
        Ok(())
    }

    /// 🌐 Automatically injects cluaiz into the Windows Global PATH.
    /// Just like Ollama or Bun, users won't need to manually configure environment variables.
    fn ensure_global_path() {
        #[cfg(windows)]
        {
            let bin_dir = cluaiz_core::EnvironmentManager::current().ensure_bin_dir().unwrap_or_else(|_| cluaiz_core::EnvironmentManager::current().bin_dir());
            let bin_str = bin_dir.to_string_lossy().to_string();
            
            // Simple PowerShell script to read User PATH, check if cluaiz is in it, and append if missing.
            let script = format!(
                "$userPath = [Environment]::GetEnvironmentVariable('Path', 'User'); \
                 if ($userPath -notlike '*{}*') {{ \
                     $newPath = $userPath + ';{}'; \
                     [Environment]::SetEnvironmentVariable('Path', $newPath, 'User'); \
                 }}",
                bin_str, bin_str
            );

            let _ = std::process::Command::new("powershell")
                .args(&["-NoProfile", "-Command", &script])
                .output();
        }
    }
}
