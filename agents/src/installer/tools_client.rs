use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use anyhow::Result;
use cluaiz_core::EnvironmentManager;
use crate::tools::registry::ToolsRegistry;

/// How long to wait for a registry or download server to accept the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest gap between received bytes before a stalled download is abandoned.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub struct ToolsInstaller;

/// Backward-compatibility alias
pub type ToolHubInstaller = ToolsInstaller;

impl ToolsInstaller {
    pub async fn install_component(component_type: &str, component_id_raw: &str) -> Result<()> {
        let (component_id, version) = if component_id_raw.contains('@') {
            let parts: Vec<&str> = component_id_raw.split('@').collect();
            (parts[0].to_string(), Some(parts[1].to_string()))
        } else {
            (component_id_raw.to_string(), None)
        };
        if !Self::is_valid_component_id(&component_id) {
            return Err(anyhow::anyhow!("Invalid {} name '{}'.", component_type, component_id_raw));
        }

        let env = EnvironmentManager::current();
        let _ = env.ensure_tools_dir();
        let component_dir = match component_type {
            "skill" => env.ensure_skills_dir().unwrap_or_else(|_| env.skills_dir()).join(&component_id),
            "plugin" => env.ensure_plugins_dir().unwrap_or_else(|_| env.plugins_dir()).join(&component_id),
            "mcp" => env.ensure_mcp_dir().unwrap_or_else(|_| env.mcp_dir()).join(&component_id),
            _ => return Err(anyhow::anyhow!("Unknown tool component type: {}", component_type)),
        };

        tracing::info!("📡 [ToolsEngine] Installing {} '{}'...", component_type, component_id);

        let registry_url_opt = Self::get_registry_url();
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()?;

        let mut download_url = String::new();
        let mut binary_download_url = String::new();
        let mut target_version = String::new();

        if let Some(registry_url) = registry_url_opt {
            let base_url = if registry_url.ends_with("/registry.json") {
                registry_url.replace("/registry.json", "")
            } else {
                let mut parts: Vec<&str> = registry_url.split('/').collect();
                parts.pop();
                parts.join("/")
            };

            let registry_resp = client.get(&registry_url).send().await;
            if let Ok(resp) = registry_resp {
                if resp.status().is_success() {
                    if let Ok(registry_json) = resp.json::<serde_json::Value>().await {
                        let route_category = if component_type == "mcp" {
                            "mcp".to_string()
                        } else {
                            format!("{}s", component_type)
                        };

                        if let Some(routing) = registry_json.get("routing").and_then(|r| r.as_object()) {
                            if let Some(family_path) = routing.get(&route_category).and_then(|p| p.as_str()) {
                                let family_url = format!("{}/{}", base_url, family_path);
                                if let Ok(family_resp) = client.get(&family_url).send().await {
                                    if family_resp.status().is_success() {
                                        if let Ok(family_json) = family_resp.json::<serde_json::Value>().await {
                                            if let Some(items) = family_json.get("items").and_then(|i| i.as_object()) {
                                                if let Some(package_path) = items.get(&component_id).and_then(|p| p.as_str()) {
                                                    let category_folder = family_path.split('/').next().unwrap_or(component_type);
                                                    let full_package_url = format!("{}/{}/{}", base_url, category_folder, package_path);

                                                    if let Ok(pkg_resp) = client.get(&full_package_url).send().await {
                                                        if pkg_resp.status().is_success() {
                                                            if let Ok(data) = pkg_resp.json::<serde_json::Value>().await {
                                                                // Check version validity if user specified a version
                                                                if let Some(ref req_ver) = version {
                                                                    if let Some(versions) = data.get("versions").and_then(|v| v.as_object()) {
                                                                        if !versions.contains_key(req_ver) {
                                                                            let available: Vec<&String> = versions.keys().collect();
                                                                            return Err(anyhow::anyhow!(
                                                                                "Version '{}' not found for '{}'. Available versions in Cluaiz Tools: {:?}",
                                                                                req_ver,
                                                                                component_id,
                                                                                available
                                                                            ));
                                                                        }
                                                                    }
                                                                }

                                                                let ver = version.clone().unwrap_or_else(|| {
                                                                    data.get("latest_version").and_then(|v| v.as_str()).unwrap_or("0.1.0").to_string()
                                                                });
                                                                target_version = ver.clone();

                                                                if let Some(versions) = data.get("versions").and_then(|v| v.as_object()) {
                                                                    if let Some(v_data) = versions.get(&ver).and_then(|v| v.as_object()) {
                                                                        if let Some(files) = v_data.get("files").and_then(|f| f.as_object()) {
                                                                            if let Some(url) = files.get("file_directory").and_then(|u| u.as_str()) {
                                                                                download_url = url.to_string();
                                                                            }
                                                                        }

                                                                        if let Some(os_obj) = v_data.get("os").and_then(|o| o.as_object()) {
                                                                            let os_key = if cfg!(target_os = "windows") {
                                                                                "windows"
                                                                            } else if cfg!(target_os = "macos") {
                                                                                "macos"
                                                                            } else {
                                                                                "linux"
                                                                            };
                                                                            if let Some(bin_url) = os_obj.get(os_key).and_then(|u| u.as_str()) {
                                                                                binary_download_url = bin_url.to_string();
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        return Err(anyhow::anyhow!(
                                                            "Could not fetch the package manifest for '{}' from '{}'.",
                                                            component_id,
                                                            full_package_url
                                                        ));
                                                    }
                                                } else {
                                                    return Err(anyhow::anyhow!(
                                                        "Component '{}' not found in Cluaiz Tools category '{}'.",
                                                        component_id,
                                                        component_type
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: Construct deterministic GitHub release URLs if not explicitly present in package.json
        if download_url.is_empty() && !component_id.is_empty() {
            let ver = if target_version.is_empty() { "1.0.0" } else { &target_version };
            let prefix = match component_type {
                "skill" => "skill",
                "plugin" => "plugin",
                "mcp" => "mcp",
                _ => "ext",
            };
            download_url = format!(
                "https://github.com/cluaiz/cluaiz-tools/releases/download/{}-{}-v{}/{}-files.zip",
                prefix, component_id, ver, component_id
            );
        }

        let binary_declared = !binary_download_url.is_empty();
        if binary_download_url.is_empty() && !component_id.is_empty() && component_type == "plugin" {
            let ver = if target_version.is_empty() { "1.0.0" } else { &target_version };
            let (os_name, ext) = if cfg!(target_os = "windows") {
                ("windows_x64", "dll")
            } else if cfg!(target_os = "macos") {
                ("macos_arm64", "dylib")
            } else {
                ("linux_x64", "so")
            };
            binary_download_url = format!(
                "https://github.com/cluaiz/cluaiz-tools/releases/download/plugin-{}-v{}/{}_{}.{}",
                component_id, ver, component_id, os_name, ext
            );
        }

        if download_url.is_empty() {
            return Err(anyhow::anyhow!(
                "Package '{}' (version '{}') does not specify a valid download bundle in Cluaiz Tools.",
                component_id,
                target_version
            ));
        }

        // 1. Download the package ZIP bundle (manifest, SKILL.md, scripts)
        let resp = client.get(&download_url).send().await
            .map_err(|e| anyhow::anyhow!("Network request failed for '{}': {}", download_url, e))?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download component bundle from '{}'. HTTP Status: {}",
                download_url,
                resp.status()
            ));
        }

        let bytes = resp.bytes().await
            .map_err(|e| anyhow::anyhow!("Failed to read downloaded bytes: {}", e))?;

        // 2. Download the pre-compiled native binary. A binary declared in the package
        //    manifest is required; the conventional release URL is only a best-effort guess.
        let mut binary_bytes = None;
        if !binary_download_url.is_empty() {
            let fetched = match client.get(&binary_download_url).send().await {
                Ok(r) if r.status().is_success() => r.bytes().await
                    .map_err(|e| anyhow::anyhow!("Failed to read native binary from '{}': {}", binary_download_url, e)),
                Ok(r) => Err(anyhow::anyhow!("Failed to download native binary from '{}'. HTTP Status: {}", binary_download_url, r.status())),
                Err(e) => Err(anyhow::anyhow!("Network request failed for '{}': {}", binary_download_url, e)),
            };
            match fetched {
                Ok(b) => binary_bytes = Some(b),
                Err(e) if binary_declared => return Err(e),
                Err(e) => tracing::debug!("No native binary for {} '{}': {}", component_type, component_id, e),
            }
        }

        // 3. Unpack into a staging folder outside the scanned category folders, then swap it
        //    in, so a failed install never leaves an empty or half-written component behind
        //    and a failed update keeps the previously installed version.
        let staging_root = env.tools_dir().join(".staging");
        std::fs::create_dir_all(&staging_root)?;
        let stamp = format!(
            "{}-{}-{}",
            component_id,
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        );
        let staging_dir = staging_root.join(&stamp);
        let backup_dir = staging_root.join(format!("{}-previous", stamp));

        let staged = Self::unpack_bundle(&bytes, binary_bytes.as_deref(), &component_id, &staging_dir)
            .and_then(|_| Self::swap_into_place(&staging_dir, &component_dir, &backup_dir));
        if let Err(e) = staged {
            if staging_dir.exists() {
                let _ = std::fs::remove_dir_all(&staging_dir);
            }
            return Err(e);
        }

        // 4. Sync with filesystem and seal tools_registry.json & tools_registry.bin.
        //    The package is already in place, so a registry problem is logged, not fatal.
        match ToolsRegistry::load() {
            Ok(mut reg) => {
                if let Err(e) = reg.sync_with_filesystem() {
                    tracing::warn!("⚠️ [ToolsEngine] Registry sync after installing '{}' failed: {}", component_id, e);
                }
                if let Err(e) = reg.save() {
                    tracing::warn!("⚠️ [ToolsEngine] Registry save after installing '{}' failed: {}", component_id, e);
                }
            }
            Err(e) => tracing::warn!("⚠️ [ToolsEngine] Registry load after installing '{}' failed: {}", component_id, e),
        }

        tracing::info!("✅ [ToolsEngine] Successfully installed {} '{}'", component_type, component_id);
        Ok(())
    }

    /// A component id must be one plain folder name, so it can never point at its
    /// category folder itself or outside it (`""`, `..`, `a/b`, drive prefixes).
    fn is_valid_component_id(id: &str) -> bool {
        let mut parts = Path::new(id).components();
        matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None))
    }

    /// Extracts the downloaded bundle (and native binary, if any) into `target_dir`.
    fn unpack_bundle(bundle: &[u8], binary: Option<&[u8]>, component_id: &str, target_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(target_dir)?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bundle))
            .map_err(|e| anyhow::anyhow!("Downloaded archive is not a valid ZIP: {}", e))?;

        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let outpath: PathBuf = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };
            if (*file.name()).ends_with('/') {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        if let Some(bin_bytes) = binary {
            let bin_ext = if cfg!(target_os = "windows") { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
            std::fs::write(target_dir.join(format!("{}.{}", component_id, bin_ext)), bin_bytes)?;
        }
        Ok(())
    }

    /// Moves a fully staged package into `component_dir`. An existing version is moved
    /// aside first and restored if the new one cannot be put in place.
    fn swap_into_place(staging_dir: &Path, component_dir: &Path, backup_dir: &Path) -> Result<()> {
        if !component_dir.exists() {
            return std::fs::rename(staging_dir, component_dir).map_err(|e| {
                anyhow::anyhow!("Failed to move the new package into '{}': {}", component_dir.display(), e)
            });
        }

        std::fs::rename(component_dir, backup_dir).map_err(|e| {
            anyhow::anyhow!("Could not move the installed version at '{}' aside (is it in use?): {}", component_dir.display(), e)
        })?;
        if let Err(e) = std::fs::rename(staging_dir, component_dir) {
            if let Err(restore) = std::fs::rename(backup_dir, component_dir) {
                return Err(anyhow::anyhow!(
                    "Failed to move the new version into '{}' ({}), and restoring the previous version from '{}' also failed ({}).",
                    component_dir.display(), e, backup_dir.display(), restore
                ));
            }
            return Err(anyhow::anyhow!(
                "Failed to move the new version into '{}'; the previous version is kept: {}",
                component_dir.display(), e
            ));
        }
        if let Err(e) = std::fs::remove_dir_all(backup_dir) {
            tracing::warn!("⚠️ [ToolsEngine] Previous version left at '{}': {}", backup_dir.display(), e);
        }
        Ok(())
    }

    pub async fn remove_component(component_type: &str, component_id: &str) -> Result<()> {
        let env = EnvironmentManager::current();
        let component_dir = match component_type {
            "skill" => env.skills_dir().join(component_id),
            "plugin" => env.plugins_dir().join(component_id),
            "mcp" => env.mcp_dir().join(component_id),
            _ => return Err(anyhow::anyhow!("Unknown tool component type: {}", component_type)),
        };

        if component_dir.exists() {
            std::fs::remove_dir_all(&component_dir)?;
        }

        if let Ok(mut reg) = ToolsRegistry::load() {
            let _ = reg.sync_with_filesystem();
            let _ = reg.save();
        }

        tracing::info!("🗑️ [ToolsEngine] Removed {} '{}'", component_type, component_id);
        Ok(())
    }

    pub fn list_installed_components(component_type: &str) -> Result<Vec<String>> {
        let env = EnvironmentManager::current();
        let base_dir = match component_type {
            "skill" => env.skills_dir(),
            "plugin" => env.plugins_dir(),
            "mcp" => env.mcp_dir(),
            _ => return Err(anyhow::anyhow!("Unknown tool component type: {}", component_type)),
        };

        let mut items = Vec::new();
        if base_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(base_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        items.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
        Ok(items)
    }

    pub fn list_component_cache(component_type: &str) -> Result<String> {
        let items = Self::list_installed_components(component_type)?;
        Ok(format!("Cached {} components: {:?}", component_type, items))
    }

    pub fn clear_component_cache(component_type: &str, target: Option<String>, _purge_logs: bool, _purge_meta: bool) -> Result<usize> {
        let items = Self::list_installed_components(component_type)?;
        let mut count = 0;
        for item in items {
            if let Some(ref t) = target {
                if &item != t {
                    continue;
                }
            }
            let _ = Self::remove_component(component_type, &item);
            count += 1;
        }
        Ok(count)
    }

    fn get_registry_url() -> Option<String> {
        let env = EnvironmentManager::current();

        let candidates: [PathBuf; 3] = [
            env.config_dir().join("package.json"),
            PathBuf::from("package.json"),
            PathBuf::from(".cluaiz/engine/config/package.json"),
        ];
        for p in &candidates {
            if p.exists() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Check "tools" first, then "hub" as backward-compatibility fallback
                        if let Some(url) = json.get("web").and_then(|w| w.get("tools").or_else(|| w.get("hub"))).and_then(|h| h.get("manifest_url")).and_then(|u| u.as_str()) {
                            return Some(url.to_string());
                        }
                        if let Some(url) = json.get("tools").or_else(|| json.get("hub")).and_then(|h| h.get("manifest_url")).and_then(|u| u.as_str()) {
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }

        Some("https://raw.githubusercontent.com/cluaiz/cluaiz-tools/main/registry.json".to_string())
    }
}
