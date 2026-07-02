use colored::Colorize;
use std::path::PathBuf;
use std::io::Write;

pub struct HubInstaller;

impl HubInstaller {
    pub async fn install_component(component_type: &str, component_id_raw: &str) -> anyhow::Result<()> {
        let (component_id, version) = if component_id_raw.contains('@') {
            let parts: Vec<&str> = component_id_raw.split('@').collect();
            (parts[0].to_string(), Some(parts[1].to_string()))
        } else {
            (component_id_raw.to_string(), None)
        };

        cluaiz_shared::dev_info!("\n  {} [Cluaiz] Contacting Universal {} Registry...", "📡".cyan(), component_type.to_uppercase());

        // Get environment directory based on type
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let component_dir = match component_type {
            "skill" => env.ensure_skills_dir().unwrap_or_else(|_| env.skills_dir()).join(&component_id),
            "extension" => env.ensure_extensions_dir().unwrap_or_else(|_| env.extensions_dir()).join(&component_id),
            "mcp" => env.ensure_mcp_dir().unwrap_or_else(|_| env.mcp_dir()).join(&component_id),
            "plugin" => env.ensure_plugins_dir().unwrap_or_else(|_| env.plugins_dir()).join(&component_id),
            _ => return Err(anyhow::anyhow!("Unknown component type: {}", component_type)),
        };

        cluaiz_shared::dev_info!("  {} [Cluaiz] Installing {} '{}'...", "🚀".green(), component_type, component_id.bold());

        let registry_url_opt = Self::get_registry_url();
        let client = reqwest::Client::new();
        
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
                        // 1. Get family path from routing
                        let route_category = if component_type == "mcp" {
                            "mcp".to_string()
                        } else {
                            format!("{}s", component_type)
                        };
                        
                        if let Some(routing) = registry_json.get("routing").and_then(|r| r.as_object()) {
                            if let Some(family_path) = routing.get(&route_category).and_then(|p| p.as_str()) {
                                
                                // 2. Fetch family.json
                                let family_url = format!("{}/{}", base_url, family_path);
                                if let Ok(family_resp) = client.get(&family_url).send().await {
                                    if family_resp.status().is_success() {
                                        if let Ok(family_json) = family_resp.json::<serde_json::Value>().await {
                                            
                                            // 3. Get package.json path from items
                                            if let Some(items) = family_json.get("items").and_then(|i| i.as_object()) {
                                                if let Some(package_path) = items.get(&component_id).and_then(|p| p.as_str()) {
                                                    
                                                    // 4. Fetch package.json
                                                    let category_folder = family_path.split('/').next().unwrap_or(component_type);
                                                    let full_package_url = format!("{}/{}/{}", base_url, category_folder, package_path);
                                                    
                                                    if let Ok(pkg_resp) = client.get(&full_package_url).send().await {
                                                        if pkg_resp.status().is_success() {
                                                            if let Ok(data) = pkg_resp.json::<serde_json::Value>().await {
                                                                
                                                                // 5. Parse versions and OS
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
        }

        if download_url.is_empty() {
            // Fallback to direct Github fetch based on user instruction
            cluaiz_shared::dev_info!("  {} [Registry] {} '{}' not found in registry JSON (or no registry configured). Attempting direct repository fetch...", "⚠️".yellow(), component_type, component_id.bold());
            
            let ver = version.unwrap_or_else(|| "main".to_string());
            target_version = ver.clone();
            
            // Construct Github archive URL
            let repo_path = if component_id.contains('/') {
                component_id.clone()
            } else {
                format!("cluaiz/{}", component_id)
            };
            
            let tag_path = if ver == "main" || ver == "master" {
                format!("refs/heads/{}", ver)
            } else {
                format!("refs/tags/v{}", ver) // Try v0.1.0 etc
            };
            
            download_url = format!("https://github.com/{}/archive/{}.zip", repo_path, tag_path);
        }

        cluaiz_shared::dev_info!("  {} [Registry] Found release: v{}", "✅".green(), target_version.bold());
        cluaiz_shared::dev_info!("  {} [Cluaiz] Downloading Master ZIP...", "⬇️".cyan());

        let zip_resp = client.get(&download_url).send().await?;
        if !zip_resp.status().is_success() {
            // let _ = std::fs::remove_dir_all(&component_dir); // REMOVED CLEANUP SO USER CAN SEE FOLDER
            return Err(anyhow::anyhow!("Failed to download package from {}", download_url));
        }
        
        let zip_bytes = zip_resp.bytes().await?;
        
        let component_dir_clone = component_dir.clone();
        let component_id_clone = component_id.replace("/", "_");
        let ctype = component_type.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if !component_dir_clone.exists() {
                std::fs::create_dir_all(&component_dir_clone)?;
            }

            let temp_zip_path = component_dir_clone.join(format!("{}.zip", component_id_clone));
            let mut file = std::fs::File::create(&temp_zip_path)?;
            file.write_all(&zip_bytes)?;
            
            cluaiz_shared::dev_info!("  {} [Cluaiz] Extracting package...", "📦".cyan());
            let status = std::process::Command::new("tar")
                .arg("-xf")
                .arg(&temp_zip_path)
                .arg("-C")
                .arg(&component_dir_clone)
                .status()?;
                
            if !status.success() {
                let _ = std::fs::remove_file(&temp_zip_path);
                // let _ = std::fs::remove_dir_all(&component_dir_clone); // REMOVED CLEANUP
                return Err(anyhow::anyhow!("Extraction failed"));
            }
            let _ = std::fs::remove_file(&temp_zip_path);
            
            if ctype == "skill" {
                 // ... vector compilation ...
            }

            Ok(())
        }).await??;

        if !binary_download_url.is_empty() {
            cluaiz_shared::dev_info!("  {} [Cluaiz] Downloading Native OS Binary...", "⚙️".cyan());
            let bin_resp = client.get(&binary_download_url).send().await?;
            if bin_resp.status().is_success() {
                let bin_bytes = bin_resp.bytes().await?;
                let file_name = binary_download_url.split('/').last().unwrap_or("binary");
                let bin_path = component_dir.join(file_name);
                let mut bin_file = std::fs::File::create(bin_path)?;
                bin_file.write_all(&bin_bytes)?;
            }
        }

        cluaiz_shared::dev_info!("\n  {} [Cluaiz] {} '{}' successfully installed and registered at: {}\n", "✅".green(), component_type, component_id.bold(), component_dir.display());

        Ok(())
    }

    pub async fn remove_component(component_type: &str, component_name: &str) -> anyhow::Result<()> {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let component_dir = match component_type {
            "skill" => env.ensure_skills_dir().unwrap_or_else(|_| env.skills_dir()).join(component_name),
            "extension" => env.ensure_extensions_dir().unwrap_or_else(|_| env.extensions_dir()).join(component_name),
            "mcp" => env.ensure_mcp_dir().unwrap_or_else(|_| env.mcp_dir()).join(component_name),
            "plugin" => env.ensure_plugins_dir().unwrap_or_else(|_| env.plugins_dir()).join(component_name),
            _ => return Err(anyhow::anyhow!("Unknown component type: {}", component_type)),
        };

        if component_dir.exists() {
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_dir_all(&component_dir);
            }).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("{} '{}' not found locally", component_type, component_name))
        }
    }

    pub fn list_installed_components(component_type: &str) -> anyhow::Result<Vec<String>> {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let target_dir = match component_type {
            "skill" => env.skills_dir(),
            "extension" => env.extensions_dir(),
            "mcp" => env.mcp_dir(),
            "plugin" => env.plugins_dir(),
            _ => return Err(anyhow::anyhow!("Unknown component type")),
        };

        let mut items = Vec::new();
        if target_dir.exists() {
            for entry in std::fs::read_dir(target_dir)? {
                if let Ok(entry) = entry {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') {
                                items.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok(items)
    }

    pub fn list_component_cache(component_type: &str) -> anyhow::Result<String> {
        Ok(format!("Cache listed for {}", component_type))
    }

    pub fn clear_component_cache(component_type: &str, _id: Option<String>, _all: bool, _force: bool) -> anyhow::Result<usize> {
        Ok(0)
    }

    fn get_registry_url() -> Option<String> {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let config_dir = env.ensure_config_dir().unwrap_or_else(|_| env.config_dir());
        
        let pkg_json_path = config_dir.join("package.json");
        
        if let Ok(pkg_data) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&pkg_data) {
                if let Some(url) = json.get("web")
                    .and_then(|w| w.get("hub"))
                    .and_then(|h| h.get("manifest_url"))
                    .and_then(|u| u.as_str()) {
                    return Some(url.to_string());
                }
            }
        }
        None
    }
}
