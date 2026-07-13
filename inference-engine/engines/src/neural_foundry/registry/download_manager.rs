use std::path::PathBuf;
use anyhow::{Result, anyhow};
use reqwest;
use std::fs;

pub struct DownloadManager;

impl DownloadManager {
    /// Gets the platform target (e.g. "win-x64", "mac-arm64", "linux-x64")
    pub fn get_platform_key() -> &'static str {
        if cfg!(windows) { 
            "win-x64" 
        } else if cfg!(target_os = "macos") { 
            "mac-arm64" 
        } else if cfg!(target_os = "android") {
            "android-arm64"
        } else { 
            "linux-x64" 
        }
    }

    /// Resolves the actual download URL by reading package.json and the component's registry.json
    pub async fn resolve_hub_url(component_type: &str, component_name: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .user_agent("cluaiz-Neural-Engine/0.1.0")
            .build()?;
        
        // 1. Read local package.json from the production config directory
        let env_manager = cluaiz_shared::environment::EnvironmentManager::current();
        let package_json_path = env_manager.config_dir().join("package.json");
        let pkg_content = std::fs::read_to_string(&package_json_path)
            .map_err(|e| anyhow!("Failed to read local package.json at {:?}: {}", package_json_path, e))?;
        let pkg: serde_json::Value = serde_json::from_str(&pkg_content)?;
        
        let manifest_url = pkg["web"][component_type]["manifest_url"]
            .as_str()
            .ok_or_else(|| anyhow!("Manifest URL not found for component type: {}", component_type))?;
            
        // 2. Fetch the specific component's registry.json
        let registry_resp = client.get(manifest_url).send().await?;
        let registry: serde_json::Value = registry_resp.json().await?;
        
        // 3. Extract the download URL for the current platform
        let platform = Self::get_platform_key();
        let download_url = registry[component_name][platform]
            .as_str()
            .ok_or_else(|| anyhow!("Download URL not found for '{}' on platform '{}'", component_name, platform))?;
            
        Ok(download_url.to_string())
    }

    /// Downloads and unpacks a component to ~/.cluaiz/<storage_domain>/<component_name>/
    pub async fn download_and_extract(url: &str, storage_domain: &str, component_name: &str) -> Result<PathBuf> {
        let base_dir = cluaiz_shared::environment::EnvironmentManager::current().global_dir.clone();
        let target_dir = base_dir.join(storage_domain).join(component_name);

        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
        }

        let client = reqwest::Client::builder()
            .user_agent("cluaiz-Neural-Engine/0.1.0")
            .build()?;

        tracing::info!("⬇️ [DownloadManager] Fetching {} from {}", component_name, url);
        
        let response = client.get(url).send().await
            .map_err(|e| anyhow!("Failed to download component: {}", e))?;
            
        let bytes = response.bytes().await?;

        // Determine if it's a zip or direct binary
        let is_zip = url.ends_with(".zip");

        if is_zip {
            let cursor = std::io::Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| anyhow::anyhow!("Zip extraction failed: {}", e))?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| anyhow::anyhow!("Failed to read zip entry: {}", e))?;
                let outpath = match file.enclosed_name() {
                    Some(path) => target_dir.join(path),
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
            // Direct binary download (e.g. .wasm, .dll)
            let dest_filename = url.split('/').last().unwrap_or("plugin.bin");
            let dest_path = target_dir.join(dest_filename);
            fs::write(&dest_path, bytes)?;
        }

        tracing::info!("✅ [DownloadManager] Successfully installed {} to {:?}", component_name, target_dir);
        Ok(target_dir)
    }
}
