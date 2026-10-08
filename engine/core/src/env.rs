//! 🌐 Centralized Environment & Path Resolution for Cluaiz Engine
//! Single Source of Truth for directories: config, models, tools, registries, logs.

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
    /// Cluaiz root directory based on the execution context.
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
        if let Ok(env_path) = std::env::var("CLUAIZ_HOME") {
            return Self {
                mode: EnvironmentMode::Installed,
                local_dir: PathBuf::from(&env_path),
                global_dir: PathBuf::from(&env_path),
            };
        }

        // 3. Development Mode: Traverse up from test/app subfolders to find master .cluaiz
        if std::env::var("CARGO").is_ok() || std::env::var("CARGO_MANIFEST_DIR").is_ok() {
            let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

            let mut check_dir = current_dir.clone();
            let mut canonical_dir = None;

            loop {
                let sibling_engine = check_dir.join("cluaiz").join(".cluaiz");
                if sibling_engine.exists() {
                    canonical_dir = Some(sibling_engine);
                    break;
                }
                let direct = check_dir.join(".cluaiz");
                let is_tauri_subfolder = check_dir.ends_with("src-tauri") || check_dir.ends_with("app");
                if direct.exists() && !is_tauri_subfolder {
                    canonical_dir = Some(direct);
                    break;
                }
                if let Some(parent) = check_dir.parent() {
                    check_dir = parent.to_path_buf();
                } else {
                    break;
                }
            }

            let resolved_local = canonical_dir.unwrap_or_else(|| home_dir.join(".cluaiz"));

            return Self {
                mode: EnvironmentMode::Development,
                local_dir: resolved_local,
                global_dir: home_dir.join(".cluaiz"),
            };
        }

        // 4. Installed Mode (Default)
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let global_path = home_dir.join(".cluaiz");
        let mgr = Self {
            mode: EnvironmentMode::Installed,
            local_dir: global_path.clone(),
            global_dir: global_path,
        };
        let _ = mgr.ensure_env_file();
        mgr.load_env();
        mgr
    }

    /// Automatically loads environment variables from ~/.cluaiz/engine/config/.env into std::env
    pub fn load_env(&self) {
        let candidates = [
            self.config_dir().join(".env"),
            self.global_dir.join("engine").join("config").join(".env"),
            self.local_dir.join("engine").join("config").join(".env"),
            std::env::current_dir().map(|p| p.join(".cluaiz/engine/config/.env")).unwrap_or_default(),
            std::env::current_dir().map(|p| p.join(".env")).unwrap_or_default(),
        ];

        for path in candidates {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        if let Some((key, val)) = trimmed.split_once('=') {
                            let key = key.trim();
                            let val = val.trim().trim_matches('"').trim_matches('\'');
                            if !key.is_empty() && std::env::var(key).is_err() {
                                std::env::set_var(key, val);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn env_file_path(&self) -> PathBuf {
        self.config_dir().join(".env")
    }

    pub fn ensure_env_file(&self) -> std::io::Result<PathBuf> {
        let path = self.env_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if !path.exists() {
            let _ = std::fs::write(&path, "# Cluaiz Engine Environment Configuration\n");
        }
        Ok(path)
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.local_dir.join("engine")
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
    pub fn ingest_models_dir(&self) -> PathBuf {
        self.models_dir().join("ingest")
    }
    pub fn embedding_models_dir(&self) -> PathBuf {
        self.models_dir().join("embedding")
    }
    pub fn tts_models_dir(&self) -> PathBuf {
        self.models_dir().join("tts")
    }
    pub fn stt_models_dir(&self) -> PathBuf {
        self.models_dir().join("stt")
    }
    pub fn kv_cache_dir(&self) -> PathBuf {
        self.local_dir.join("kv_cache")
    }
    pub fn tools_dir(&self) -> PathBuf {
        self.global_dir.join("tools")
    }
    pub fn skills_dir(&self) -> PathBuf {
        self.tools_dir().join("skills")
    }
    pub fn plugins_dir(&self) -> PathBuf {
        self.tools_dir().join("plugins")
    }
    pub fn mcp_dir(&self) -> PathBuf {
        self.tools_dir().join("mcp")
    }
    pub fn tools_registry_json_path(&self) -> PathBuf {
        let p = self.config_dir().join("tools_registry.json");
        if p.exists() {
            p
        } else {
            let global_p = self.global_dir.join("engine").join("config").join("tools_registry.json");
            if global_p.exists() {
                global_p
            } else {
                p
            }
        }
    }
    pub fn model_registry_json_path(&self) -> PathBuf {
        let p = self.config_dir().join("model_registry.json");
        if p.exists() {
            p
        } else {
            let global_p = self.global_dir.join("engine").join("config").join("model_registry.json");
            if global_p.exists() {
                global_p
            } else {
                p
            }
        }
    }
    pub fn reports_dir(&self) -> PathBuf {
        self.local_dir.join("reports")
    }

    pub fn ensure_engine_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.engine_dir();
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
        Ok(dir)
    }

    pub fn ensure_models_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.models_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }
}
