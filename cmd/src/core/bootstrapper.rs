use std::path::{Path, PathBuf};
use cluaiz_shared::HardwareGovernor;
use color_eyre::{Result, eyre::eyre};
use colored::Colorize;

pub struct Bootstrapper;

impl Bootstrapper {
    const MASTER_REGISTRY_URL: &'static str = "https://raw.githubusercontent.com/cluaiz/cluaiz/main/package.json";

    /// 🚀 cluaiz BOOTSTRAP: The Sovereign Handshake.
    pub async fn ignite(is_dev_sync: bool) -> Result<()> {
        let local_dir = cluaiz_shared::environment::EnvironmentManager::current().local_dir;
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        let _ = Self::sync_dev_artifacts("all", None, local_dir, profile);
        Self::ensure_global_path();
        
        // 🚀 0. Neural Foundry Genesis (Create Permission.json and Trigger Compiler Daemons)
        tracing::info!("🧠 [cluaiz] Igniting Neural Foundry (Permissions & Skills)...");
        let mut permissions = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        permissions.auto_assign_defaults();
        
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let mut registry = engines::neural_foundry::registry::SkillRegistry::new();
        for dir in [env.skills_dir(), env.extensions_dir(), env.plugins_dir(), env.mcp_dir()] {
            if dir.exists() {
                registry.load_from_directory(&dir.to_string_lossy());
            }
        }

        // 📜 Write Third-Party Licenses (Automatic Legal Compliance)
        let hub_path = cluaiz_shared::HardwareGovernor::resolve_hub_path();
        let _ = std::fs::create_dir_all(&hub_path);
        let license_text = include_str!("../assets/THIRD_PARTY_NOTICES.txt");
        let _ = std::fs::write(hub_path.join("THIRD_PARTY_LICENSES.txt"), license_text);

        #[cfg(windows)]
        let _ = colored::control::set_virtual_terminal(true);

        let bin_dir = HardwareGovernor::resolve_hub_path().join("bin");

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
        cluaiz_shared::RegistryGovernor::seal_registry(master_registry.clone())
            .map_err(|e| eyre!("Binary Truth Seal Error: {}", e))?;
        tracing::debug!("✅ [Registry] Binary Truth sealed and verified.");

        // 🎯 2. CLI Lifecycle Check
        let cli_info = &master_registry["components"]["cli"];
        let latest_cli = cli_info["version"].as_str().unwrap_or("");
        let current_cli = env!("CARGO_PKG_VERSION");
        
        if latest_cli != current_cli && !latest_cli.is_empty() {
            println!("  {} [cluaiz] Update Available: {} -> {}", "🚀".green(), current_cli, latest_cli);
        }

        // 🎯 3. Engine Sync (Driven by package.json)
        let engine_info = &master_registry["components"]["engine"];
        let engine_dir = HardwareGovernor::resolve_engine_path();
        let ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
        let engine_path = engine_dir.join(format!("cluaiz-engine.{}", ext));
        let engine_marker = engine_dir.join("cluaiz-engine.ready");

        let manifest_version = engine_info["version"].as_str().unwrap_or("unknown");
        let local_version = std::fs::read_to_string(&engine_marker).unwrap_or_default();

        if !engine_path.exists() || local_version != manifest_version {
            println!("  {} [cluaiz] Provisioning Core Engine ({})...", "⚙️".yellow(), manifest_version);
            let manifest_url = engine_info["manifest_url"].as_str().ok_or_else(|| eyre!("Engine Manifest URL missing."))?;
            
            match async {
                let res = client.get(manifest_url).send().await?;
                if !res.status().is_success() {
                    return Err(eyre!("Registry Error: {} returned {}", manifest_url, res.status()));
                }
                let engine_manifest: serde_json::Value = res.json().await?;
                Self::download_engine_with_manifest(&engine_path, &engine_manifest).await?;
                std::fs::write(&engine_marker, manifest_version)?;
                Ok::<(), color_eyre::Report>(())
            }.await {
                Ok(_) => {},
                Err(e) if engine_path.exists() => {
                    println!("  {} [Cluaiz] Provisioning failed ({}). Using cached engine.", "⚠️".yellow(), e);
                }
                Err(e) => {
                    return Err(eyre!("{}\n\n💡 Dev Hint: The engine binary is missing and network provisioning failed. Please run:\n   cargo run -- dev-sync all\nto install your locally compiled artifacts.", e));
                }
            }
        }

        // 🎯 4. Kernel & Stack Sync
        Self::sync_neural_stack(&master_registry).await?;



        Ok(())
    }

    async fn download_engine_with_manifest(dest: &Path, manifest: &serde_json::Value) -> Result<()> {
        let platform = if cfg!(windows) { "win-x64" } else if cfg!(target_os = "macos") { "mac-arm64" } else { "linux-x64" };
        
        let url_option = manifest["engines"][platform].as_str();
        
        if let Some(url) = url_option {
            Self::download_asset(url, dest).await?;
        } else if dest.exists() {
            tracing::warn!("⚠️ [Bootstrapper] Platform '{}' not found in Engine Registry, but local engine exists. Skipping download.", platform);
        } else {
            return Err(eyre!("Platform '{}' not found in Engine Registry and no local engine found.", platform));
        }
        
        Ok(())
    }

    async fn sync_neural_stack(master_registry: &serde_json::Value) -> Result<()> {
        let control_path = HardwareGovernor::resolve_engine_path().join("system_control.json");
        if !control_path.exists() {
            return Ok(());
        } 

        let control_data: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&control_path)?)?;
        let has_nvidia = control_data["silicon_truth"]["accelerators"]["gpus"]
            .as_array()
            .map(|gpus| gpus.iter().any(|g| g["vendor"].as_str().map(|v| v.to_uppercase()).unwrap_or_default().contains("NVIDIA")))
            .unwrap_or(false);

        let platform = if cfg!(windows) { "win-x64" } else if cfg!(target_os = "macos") { "mac-arm64" } else { "linux-x64" };

        // Kernel Sync (Version-Aware via package.json)
        let kernel_info = &master_registry["components"]["kernel"];
        let kernel_dir = HardwareGovernor::resolve_interface_path();
        let kernel_ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
        let kernel_path = kernel_dir.join(format!("cluaiz-llama.{}", kernel_ext));
        let kernel_marker = kernel_dir.join("cluaiz-llama.ready");

        let manifest_version = kernel_info["version"].as_str().unwrap_or("unknown");
        let local_version = std::fs::read_to_string(&kernel_marker).unwrap_or_default();

        if !kernel_path.exists() || local_version != manifest_version {
            println!("  {} [cluaiz] Synchronizing Neural Kernel ({})...", "📦".magenta(), manifest_version);
            let client = reqwest::Client::builder().user_agent(format!("cluaiz-Bootstrapper/{}", env!("CARGO_PKG_VERSION"))).build()?;
            let manifest_url = kernel_info["manifest_url"].as_str().ok_or_else(|| eyre!("Kernel Manifest URL missing."))?;
            
            match async {
                let res = client.get(manifest_url).send().await?;
                if !res.status().is_success() {
                    return Err(eyre!("Registry Error: {} returned {}", manifest_url, res.status()));
                }
                let manifest: serde_json::Value = res.json().await?;

                let mut spec_key = platform.to_string();
                if platform == "win-x64" || platform == "linux-x64" {
                    let isa_features = control_data["silicon_truth"]["cpu"]["isa_features"].as_array();
                    let has_avx512 = isa_features.map(|feats| feats.iter().any(|f| f.as_str() == Some("AVX-512"))).unwrap_or(false);
                    spec_key = if has_avx512 { format!("{}-avx512", platform) } else { format!("{}-avx2", platform) };
                }

                let url_option = manifest["kernels"][&spec_key].as_str().or_else(|| manifest["kernels"][platform].as_str());
                
                if let Some(url) = url_option {
                    Self::download_asset(url, &kernel_path).await?;
                } else if kernel_path.exists() {
                    tracing::warn!("⚠️ [Bootstrapper] Kernel key '{}' not found in registry, but local kernel exists. Skipping.", spec_key);
                } else {
                    return Err(eyre!("Kernel key '{}' not found and no local kernel found.", spec_key));
                }
                std::fs::write(&kernel_marker, manifest_version)?;
                Ok::<(), color_eyre::Report>(())
            }.await {
                Ok(_) => {},
                Err(e) if kernel_path.exists() => {
                    println!("  {} [Cluaiz] Kernel sync failed ({}). Using cached kernel.", "⚠️".yellow(), e);
                }
                Err(e) => {
                    return Err(eyre!("{}\n\n💡 Dev Hint: The neural kernel is missing and network provisioning failed. Please run:\n   cargo run -- dev-sync all\nto install your locally compiled artifacts.", e));
                }
            }
        }

        if has_nvidia {
            let driver_manifest_url = master_registry["components"]["drivers"]["manifest_url"].as_str().unwrap_or_default();
            let _ = engines::interface_engines::manager::driver_provisioner::DriverProvisioner::provision_for_hardware("cuda", driver_manifest_url).await;
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

        // 1. Engine Sync (engines.dll -> cluaiz-engine.dll)
        if target == "all" || target == "core" {
            let engine_dest = hub_path.join("engine").join(format!("cluaiz-engine.{}", ext));
            
            if let Some((engine_src, is_release)) = find_artifact("engines", ext) {
                let _ = std::fs::create_dir_all(engine_dest.parent().unwrap());
                if let Err(e) = copy_with_rename(&engine_src, &engine_dest) {
                    tracing::warn!("⚠️ [DevSync] Engine Link Failed: {}. (File might be locked by another process)", e);
                } else {
                    let marker = "dev-release";
                    let _ = std::fs::write(hub_path.join("engine").join("cluaiz-engine.ready"), marker);
                    tracing::info!("🧬 [DevSync] Engine Linked (release={}): {:?}", is_release, engine_dest);
                }
            } else {
                tracing::error!("❌ [DevSync] `engines.{}` NOT FOUND! Please run `cargo build --workspace` first!", ext);
            }
        }

        // 2. Kernel Sync
        let kernels_to_sync = vec![
            ("cluaiz_llama", "cluaiz-llama"),
            ("cluaiz_onnx", "cluaiz-onnx"),
            ("onnxruntime", "onnxruntime"),
            ("onnxruntime_providers_shared", "onnxruntime_providers_shared"),
            ("onnxruntime_providers_cuda", "onnxruntime_providers_cuda"),
            ("onnxruntime_providers_tensorrt", "onnxruntime_providers_tensorrt"),
            ("onnxruntime_providers_nv_tensorrt_rtx", "onnxruntime_providers_nv_tensorrt_rtx"),
        ];

        let interface_path = hub_path.join("engine");

        for (src_name, dest_name) in kernels_to_sync {
            // Apply filtering logic
            if target == "core" {
                continue; // Skip drivers if only core
            } else if target == "driver" {
                if let Some(d) = driver_name {
                    if !dest_name.contains(d) && !src_name.contains(d) {
                        continue; // Skip if it doesn't match the requested driver
                    }
                }
            }

            if let Some((kernel_src, is_kernel_release)) = find_artifact(src_name, ext) {
                let kernel_dest = if src_name.starts_with("onnxruntime") {
                    interface_path.join("drivers").join(format!("{}.{}", dest_name, ext))
                } else {
                    interface_path.join(format!("{}.{}", dest_name, ext))
                };
                
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
                    tracing::info!("🧬 [DevSync] {} Kernel Linked (release={}): {:?}", dest_name, is_kernel_release, kernel_dest);
                }
            } else {
                if target == "drivers" || target == "driver" {
                    tracing::warn!("⚠️ [DevSync] Kernel source not found for: {}", src_name);
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
            let bin_dir = cluaiz_shared::HardwareGovernor::resolve_bin_gateway();
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
