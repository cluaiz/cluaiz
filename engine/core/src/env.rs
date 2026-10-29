//! 🌐 Centralized Environment & Path Resolution for Cluaiz Engine
//! Single Source of Truth for directories: config, models, tools, registries, logs.

use std::path::{Path, PathBuf};

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
                    let mgr = Self {
                        mode: EnvironmentMode::Portable,
                        local_dir: parent.to_path_buf(),
                        global_dir: parent.to_path_buf(),
                    };
                    mgr.migrate_engine_folder();
                    return mgr;
                }
            }
        }

        // 2. Environment Override
        if let Ok(env_path) = std::env::var("CLUAIZ_HOME") {
            let mgr = Self {
                mode: EnvironmentMode::Installed,
                local_dir: PathBuf::from(&env_path),
                global_dir: PathBuf::from(&env_path),
            };
            mgr.migrate_engine_folder();
            return mgr;
        }

        // 3. Central System Directory (Single Source of Truth)
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let global_path = home_dir.join(".cluaiz");
        let mode = if std::env::var("CARGO").is_ok() || std::env::var("CARGO_MANIFEST_DIR").is_ok() {
            EnvironmentMode::Development
        } else {
            EnvironmentMode::Installed
        };

        let mgr = Self {
            mode,
            local_dir: global_path.clone(),
            global_dir: global_path,
        };
        mgr.migrate_engine_folder();
        let _ = mgr.ensure_env_file();
        mgr.load_env();
        mgr
    }

    /// Automatically loads environment variables from ~/.cluaiz/config/.env into std::env
    pub fn load_env(&self) {
        let candidates = [
            self.config_dir().join(".env"),
            self.global_dir.join("config").join(".env"),
            std::env::current_dir().map(|p| p.join(".cluaiz/config/.env")).unwrap_or_default(),
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

    /// Settings, permissions, registries and the cached hardware scan
    pub fn config_dir(&self) -> PathBuf {
        self.local_dir.join("config")
    }
    /// Every driver is beside `cluaiz`: the GPU packs the installer puts there, and the NVIDIA
    /// libraries the engine downloads for ONNX models on the GPU (cuDNN, cuFFT, cuRAND)
    pub fn drivers_dir(&self) -> PathBuf {
        self.bin_dir()
    }
    /// The native media package (OCR, PDF rendering, video decoding), when installed
    pub fn media_dir(&self) -> PathBuf {
        self.bin_dir().join("media")
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
            let global_p = self.global_dir.join("config").join("tools_registry.json");
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
            let global_p = self.global_dir.join("config").join("model_registry.json");
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

    pub fn ensure_tools_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.tools_dir();
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

    /// Configuration is JSON only. Old binary caches (`*.bin` in the config folder) are moved to
    /// `backups/bin-<date>/` (never deleted); returns the files moved.
    pub fn retire_config_bins(&self) -> Vec<PathBuf> {
        let date = chrono::Local::now().format("%Y%m%d").to_string();
        retire_bins(&self.config_dir(), &self.local_dir.join("backups").join(format!("bin-{date}")))
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.local_dir.join("bin")
    }

    pub fn ensure_bin_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.bin_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// Up to v0.1.1 the home had an `engine/` folder: settings in `engine/config`, downloaded NVIDIA
    /// libraries in `engine/drivers`, the native media package in `engine/media` and VRAM
    /// measurements in `engine/vram_margins.json`. They move once, at the first start of a newer
    /// build, to `config/`, `bin/`, `bin/media/` and `config/`. An item already at its new place is
    /// left in `engine/` (nothing is overwritten); `engine/` is removed when it is empty.
    fn migrate_engine_folder(&self) {
        static DONE: std::sync::Once = std::sync::Once::new();
        DONE.call_once(|| migrate_engine_folder(&self.local_dir));
    }
}

fn migrate_engine_folder(home: &Path) {
    let engine = home.join("engine");
    if !engine.is_dir() {
        return;
    }
    let move_entries = |from: &Path, to: &Path| {
        let Ok(entries) = std::fs::read_dir(from) else { return };
        if std::fs::create_dir_all(to).is_err() {
            return;
        }
        for entry in entries.flatten() {
            let target = to.join(entry.file_name());
            // A library a running Cluaiz has loaded cannot move now: it moves at a later start
            if !target.exists() {
                let _ = std::fs::rename(entry.path(), &target);
            }
        }
        let _ = std::fs::remove_dir(from);
    };
    move_entries(&engine.join("config"), &home.join("config"));
    move_entries(&engine.join("drivers"), &home.join("bin"));
    let (media, new_media) = (engine.join("media"), home.join("bin").join("media"));
    if media.is_dir() && !new_media.exists() && std::fs::create_dir_all(home.join("bin")).is_ok() {
        let _ = std::fs::rename(&media, &new_media);
    }
    let (margins, new_margins) = (engine.join("vram_margins.json"), home.join("config").join("vram_margins.json"));
    if margins.is_file() && !new_margins.exists() && std::fs::create_dir_all(home.join("config")).is_ok() {
        let _ = std::fs::rename(&margins, &new_margins);
    }
    let _ = std::fs::remove_dir(&engine);
}

/// Moves every `*.bin` file of `dir` into `backup` (created when needed); returns the new paths
fn retire_bins(dir: &Path, backup: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut moved = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let Some(name) = path.file_name() else { continue };
        if std::fs::create_dir_all(backup).is_err() {
            break;
        }
        let target = backup.join(name);
        let done = std::fs::rename(&path, &target).is_ok()
            || (std::fs::copy(&path, &target).is_ok() && std::fs::remove_file(&path).is_ok());
        if done {
            moved.push(target);
        }
    }
    moved
}

#[cfg(test)]
mod bin_tests {
    use super::*;

    #[test]
    fn the_engine_folder_moves_to_config_and_bin_once() {
        let home = std::env::temp_dir().join(format!("cluaiz-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let engine = home.join("engine");
        for dir in ["config", "drivers", "media"] {
            std::fs::create_dir_all(engine.join(dir)).unwrap();
        }
        std::fs::write(engine.join("config").join("settings.json"), b"{}").unwrap();
        std::fs::write(engine.join("drivers").join("cudnn64_9.dll"), b"x").unwrap();
        std::fs::write(engine.join("media").join("native-assets.json"), b"{}").unwrap();
        std::fs::write(engine.join("vram_margins.json"), b"{}").unwrap();
        // The installer already put the GPU pack in bin: it stays
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join("ggml-cuda.dll"), b"pack").unwrap();

        migrate_engine_folder(&home);
        assert!(home.join("config").join("settings.json").is_file());
        assert!(home.join("config").join("vram_margins.json").is_file());
        assert!(home.join("bin").join("cudnn64_9.dll").is_file());
        assert!(home.join("bin").join("ggml-cuda.dll").is_file());
        assert!(home.join("bin").join("media").join("native-assets.json").is_file());
        assert!(!engine.exists());

        // An item already at its new place is not overwritten, and engine/ stays for it
        std::fs::create_dir_all(engine.join("config")).unwrap();
        std::fs::write(engine.join("config").join("settings.json"), b"old").unwrap();
        migrate_engine_folder(&home);
        assert_eq!(std::fs::read(home.join("config").join("settings.json")).unwrap(), b"{}");
        assert!(engine.join("config").join("settings.json").is_file());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn bins_move_to_the_backup_and_json_stays() {
        let root = std::env::temp_dir().join(format!("cluaiz-bins-{}", std::process::id()));
        let (config, backup) = (root.join("config"), root.join("backups").join("bin-test"));
        std::fs::create_dir_all(&config).unwrap();
        for name in ["package.bin", "permission.bin", "package.json"] {
            std::fs::write(config.join(name), b"x").unwrap();
        }
        let moved = retire_bins(&config, &backup);
        assert_eq!(moved.len(), 2);
        assert!(!config.join("package.bin").exists() && !config.join("permission.bin").exists());
        assert!(config.join("package.json").exists());
        assert!(backup.join("package.bin").exists() && backup.join("permission.bin").exists());
        // Nothing left to move: the backup is not created again
        assert!(retire_bins(&config, &root.join("backups").join("bin-again")).is_empty());
        assert!(!root.join("backups").join("bin-again").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
