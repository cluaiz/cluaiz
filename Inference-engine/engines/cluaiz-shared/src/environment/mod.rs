use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentMode {
    Development,
    Installed,
    Portable,
    Testing,
}

#[derive(Debug, Clone)]
pub struct EnvironmentManager {
    pub mode: EnvironmentMode,
    pub local_dir: PathBuf,
    pub global_dir: PathBuf,
}

impl EnvironmentManager {
    /// Returns the current global environment manager, dynamically resolving the correct
    /// cluaiz root directory based on the execution context.
    pub fn current() -> Self {
        // 1. Portable Mode: Ignore OS HOME if portable.flag exists next to the exe
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                if parent.join("portable.flag").exists() {
                    return Self {
                        mode: EnvironmentMode::Portable,
                        local_dir: parent.to_path_buf(),
                        global_dir: parent.to_path_buf(),
                    };
                }
            }
        }

        // 2. Environment Override
        if let Ok(env_path) = std::env::var("cluaiz_HOME") {
            return Self {
                mode: EnvironmentMode::Installed,
                local_dir: PathBuf::from(&env_path),
                global_dir: PathBuf::from(&env_path),
            };
        }

        // 3. Development Mode
        // We detect if we're running via cargo
        if std::env::var("CARGO").is_ok() || std::env::var("CARGO_MANIFEST_DIR").is_ok() {
            let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            return Self {
                mode: EnvironmentMode::Development,
                local_dir: current_dir.join(".cluaiz"),
                global_dir: home_dir.join(".cluaiz"),
            };
        }

        // 4. Installed Mode (Default)
        // Check dirs package for home directory
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let global_path = home_dir.join(".cluaiz");
        Self {
            mode: EnvironmentMode::Installed,
            local_dir: global_path.clone(),
            global_dir: global_path,
        }
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.local_dir.join("engine")
    }
    pub fn kernel_dir(&self) -> PathBuf {
        self.engine_dir()
    }
    pub fn drivers_dir(&self) -> PathBuf {
        self.engine_dir().join("drivers")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.engine_dir().join("config")
    }
    pub fn models_dir(&self) -> PathBuf {
        self.global_dir.join("models")
    }
    pub fn chat_models_dir(&self) -> PathBuf {
        self.models_dir().join("chat")
    }
    pub fn embedding_models_dir(&self) -> PathBuf {
        self.models_dir().join("embedding")
    }
    pub fn vision_models_dir(&self) -> PathBuf {
        self.models_dir().join("vision")
    }
    pub fn kv_cache_dir(&self) -> PathBuf {
        self.local_dir.join("kv_cache")
    }
    pub fn skills_dir(&self) -> PathBuf {
        self.global_dir.join("skills")
    }
    pub fn extensions_dir(&self) -> PathBuf {
        self.global_dir.join("extensions")
    }
    pub fn plugins_dir(&self) -> PathBuf {
        self.global_dir.join("plugins")
    }
    pub fn mcp_dir(&self) -> PathBuf {
        self.global_dir.join("mcp")
    }
    pub fn reports_dir(&self) -> PathBuf {
        self.local_dir.join("reports")
    }

    pub fn ensure_kv_cache_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.kv_cache_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_engine_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.engine_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_kernel_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.kernel_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_drivers_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.drivers_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_config_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.config_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        
        // GAP C FIX: Legacy Config Migration Block
        let engine_dir = self.engine_dir();
        let legacy_files = vec![
            "Permission.json", "Permission.bin", 
            "system_control.json", "system_control.bin",
            "package.json", "package.bin"
        ];
        
        for file in legacy_files {
            let legacy_path = engine_dir.join(file);
            let new_path = dir.join(file);
            if legacy_path.exists() {
                if !new_path.exists() {
                    if let Err(e) = std::fs::copy(&legacy_path, &new_path) {
                        tracing::warn!("⚠️ Failed to migrate legacy config {}: {}", file, e);
                    } else {
                        let _ = std::fs::remove_file(&legacy_path);
                        tracing::info!("✅ Migrated legacy config {} to {:?}", file, new_path);
                    }
                } else {
                    // New path already exists, just clean up the legacy zombie file
                    let _ = std::fs::remove_file(&legacy_path);
                    tracing::info!("🧹 Cleaned up legacy zombie config {}", file);
                }
            }
        }
        
        Ok(dir)
    }

    pub fn ensure_models_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.models_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_chat_models_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.chat_models_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_embedding_models_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.embedding_models_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_vision_models_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.vision_models_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }



    pub fn ensure_skills_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.skills_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_extensions_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.extensions_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_plugins_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.plugins_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn ensure_mcp_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.mcp_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }
}
