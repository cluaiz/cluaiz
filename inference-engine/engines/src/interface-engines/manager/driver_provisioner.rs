use std::path::PathBuf;
use anyhow::{Result, anyhow};
use reqwest;
use std::fs;
use cluaiz_shared::HardwareGovernor;
use colored::Colorize;

pub struct DriverProvisioner;

impl DriverProvisioner {
    /// 🛠️ Construct Registry Key: Dynamically maps local hardware details to the registry's flat keys.
    fn get_registry_key(driver_type: &str) -> String {
        let platform = if cfg!(windows) { 
            "win-x64" 
        } else if cfg!(target_os = "macos") { 
            "mac-arm64" 
        } else if cfg!(target_os = "android") {
            "android-arm64"
        } else { 
            "linux-x64" 
        };

        match driver_type {
            "cuda" => format!("{}-cuda-12", platform),
            "rocm" | "hip" => format!("{}-{}", platform, driver_type),
            "vulkan" => format!("{}-vulkan", platform),
            "openvino" => format!("{}-openvino", platform),
            "cann" => format!("{}-cann", platform),
            "qnn" => format!("{}-qnn", platform),
            "metal" => format!("{}-metal", platform),
            _ => format!("{}-{}", platform, driver_type),
        }
    }

    /// 🛠️ Provision Kernel Binary: Auto-detects and deploys specialized engine kernels (llama-cuda, etc.)
    pub async fn provision_kernel(kernel_type: &str, backend: &str, manifest_url: &str) -> Result<PathBuf> {
        let kernel_dir = HardwareGovernor::resolve_interface_path();
        
        if !kernel_dir.exists() {
            fs::create_dir_all(&kernel_dir)?;
        }

        let registry_key = Self::get_registry_key(backend);
        let binary_id = format!("{}-{}", kernel_type, backend);
        let marker = kernel_dir.join(format!("{}.ready", kernel_type));

        let ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
        let dest_filename = format!("cluaiz-{}.{}", kernel_type, ext);
        let dest_path = kernel_dir.join(&dest_filename);

        // 🛡️ SOVEREIGN GUARD: If a locally-built CUDA kernel exists (>30MB), NEVER overwrite it
        // with a downloaded CPU-only kernel from GitHub (typically 8-20MB).
        // A large local DLL means it was built with --features cuda and has CUDA kernels baked in.
        if dest_path.exists() {
            let size_mb = dest_path.metadata().map(|m| m.len()).unwrap_or(0) / (1024 * 1024);
            if size_mb > 30 {
                tracing::info!("🛡️ [Provisioner] CUDA-linked kernel detected ({} MB). Skipping GitHub overwrite.", size_mb);
                cluaiz_shared::dev_info!("  {} [Provisioner] Sovereign CUDA kernel preserved ({} MB). Skipping registry sync.", "🛡️".green(), size_mb);
                return Ok(dest_path);
            }
        }

        let client = reqwest::Client::builder()
            .user_agent("cluaiz-Neural-Engine/0.1.0")
            .build()?;

        let response = client.get(manifest_url).send().await
            .map_err(|e| anyhow!("Registry Sync Failed: {}", e))?;

        let text = response.text().await?;
        let text = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let manifest: serde_json::Value = serde_json::from_str(&text)?;
        
        let manifest_version = manifest["version"].as_str().unwrap_or("unknown");

        if marker.exists() {
            let local_version = fs::read_to_string(&marker).unwrap_or_default();
            if local_version == manifest_version {
                let p = kernel_dir.join(format!("cluaiz-{}.{}", kernel_type, ext));
                if p.exists() { return Ok(p); }
            }
        }

        cluaiz_shared::dev_info!("  {} [PROVISIONER] Missing Neural Kernel '{}'. Provisioning from Registry...", "🧬".cyan(), kernel_type);

        let download_url = manifest["kernel"][kernel_type][&registry_key]
            .as_str()
            .ok_or_else(|| anyhow!("Kernel '{}' for platform '{}' not found.", kernel_type, registry_key))?;

        let bin_response = client.get(download_url).send().await?;
        let bytes = bin_response.bytes().await?;
        fs::write(&dest_path, bytes)?;

        fs::write(marker, manifest_version)?;
        cluaiz_shared::dev_info!("  {} [PROVISIONER] Kernel '{}' successfully deployed.", "✅".green(), kernel_type);

        Ok(dest_path)
    }


    /// 🛠️ Provision Hardware Driver: Auto-detects, downloads, and deploys missing or stale silicon drivers.
    pub async fn provision_for_hardware(driver_type: &str, manifest_url: &str) -> Result<()> {
        let driver_dir = HardwareGovernor::resolve_interface_path().join("drivers");
        
        if !driver_dir.exists() {
            fs::create_dir_all(&driver_dir)?;
        }

        let client = reqwest::Client::builder().user_agent("cluaiz-Neural-Engine/0.1.0").build()?;
        let response = client.get(manifest_url).send().await?;
        
        let text = response.text().await?;
        let text = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let manifest: serde_json::Value = serde_json::from_str(&text)?;
        
        let manifest_version = manifest["version"].as_str().unwrap_or("unknown");

        let marker = driver_dir.join(format!("{}.ready", driver_type));
        if marker.exists() {
            let local_version = fs::read_to_string(&marker).unwrap_or_default();
            if local_version == manifest_version {
                return Ok(());
            }
        }

        cluaiz_shared::dev_info!("  {} [PROVISIONER] Provisioning Silicon Driver: {}...", "⚙️".yellow(), driver_type);
        let registry_key = Self::get_registry_key(driver_type);
        let download_url = manifest["drivers"][&registry_key].as_str()
            .ok_or_else(|| anyhow!("Driver key '{}' not found.", registry_key))?;

        let dest_filename = download_url.split('/').last().unwrap_or("driver.bin");
        let dest_path = driver_dir.join(dest_filename);

        let bin_response = client.get(download_url).send().await?;
        let bytes = bin_response.bytes().await?;
        
        if dest_filename.ends_with(".zip") {
            let cursor = std::io::Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| anyhow::anyhow!("Zip extraction failed: {}", e))?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| anyhow::anyhow!("Failed to read zip entry: {}", e))?;
                let outpath = match file.enclosed_name() {
                    Some(path) => driver_dir.join(path),
                    None => continue,
                };
                if file.name().ends_with('/') {
                    fs::create_dir_all(&outpath)?;
                } else {
                    if let Some(p) = outpath.parent() {
                        if !p.exists() {
                            fs::create_dir_all(p)?;
                        }
                    }
                    let mut outfile = fs::File::create(&outpath)?;
                    std::io::copy(&mut file, &mut outfile)?;
                }
            }
        } else {
            fs::write(&dest_path, bytes)?;
        }

        fs::write(marker, manifest_version)?;
        Ok(())
    }

    pub fn discover_system_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.push(Self::get_driver_path());

        #[cfg(target_os = "windows")]
        {
            if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
                let bin_path = PathBuf::from(cuda_path).join("bin");
                if bin_path.exists() {
                    paths.push(bin_path);
                }
            }
        }
        paths
    }

    pub fn get_driver_path() -> PathBuf {
        HardwareGovernor::resolve_hub_path().join("interface-engines").join("drivers")
    }
}
