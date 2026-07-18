use std::path::PathBuf;
use std::fs;
use crate::environment::EnvironmentManager;

/// The Centralized Configuration Manager.
/// Provides a zero-copy fast path (via `rkyv`) and a JSON fallback for user edits.
pub struct ConfigManager;

impl ConfigManager {
    /// Resolves the global config directory (`~/.cluaiz/engine/config/`)
    pub fn config_dir() -> PathBuf {
        EnvironmentManager::current().engine_dir().join("config")
    }
}


/// A macro to automatically generate `load()` and `save()` methods for any Config Schema.
/// It avoids the complex generic bounds of `rkyv` while centralizing the I/O logic.
#[macro_export]
macro_rules! define_config {
    ($struct_name:ident, $file_stem:expr) => {
        impl $struct_name {
            /// Loads the configuration, prioritizing the fast `.bin` zero-copy memory map.
            /// Falls back to `.json` if the `.bin` is missing or corrupted.
            pub fn load() -> Self {
                let base = $crate::environment::config_manager::ConfigManager::config_dir();
                let bin_path = base.join(format!("{}.bin", $file_stem));
                let json_path = base.join(format!("{}.json", $file_stem));

                // 🚀 Priority 1: Binary Truth (Panic-Safe Rkyv Zero-Copy)
                if bin_path.exists() {
                    if let Ok(bytes_raw) = std::fs::read(&bin_path) {
                        let mut bytes = rkyv::AlignedVec::with_capacity(bytes_raw.len());
                        bytes.extend_from_slice(&bytes_raw);
                        {
                            let result = std::panic::catch_unwind(|| {
                                if bytes.len() < 32 { return None; }
                                let archived = unsafe { rkyv::archived_root::<Self>(&bytes) };
                                rkyv::Deserialize::deserialize(archived, &mut rkyv::Infallible).ok()
                            });

                            if let Ok(Some(control)) = result {
                                return control;
                            }
                        }
                        // If panic or error, wipe it
                        let _ = std::fs::remove_file(&bin_path);
                    }
                }

                // 🛡️ Priority 2: JSON Fallback (User Editable Truth)
                if json_path.exists() {
                    if let Ok(data) = std::fs::read_to_string(&json_path) {
                        match serde_json::from_str::<Self>(&data) {
                            Ok(control) => {
                                // Always sync to binary truth to keep .bin updated in real-time
                                let _ = control.save();
                                return control;
                            }
                            Err(e) => {
                                tracing::warn!("❌ Failed to parse {}.json: {}. Using default.", $file_stem, e);
                            }
                        }
                    } else {
                        tracing::warn!("❌ Failed to read {}.json. Using default.", $file_stem);
                    }
                }

                // Default fallback if neither exists
                Self::default()
            }

            /// Saves the configuration atomically to both `.json` and `.bin` formats.
            pub fn save(&self) -> anyhow::Result<()> {
                let base = $crate::environment::config_manager::ConfigManager::config_dir();
                if !base.exists() {
                    std::fs::create_dir_all(&base)?;
                }

                let bin_path = base.join(format!("{}.bin", $file_stem));
                let json_path = base.join(format!("{}.json", $file_stem));

                let temp_bin = base.join(format!("{}.bin.tmp", $file_stem));
                let temp_json = base.join(format!("{}.json.tmp", $file_stem));

                // Write JSON
                let json_str = serde_json::to_string_pretty(self)?;
                std::fs::write(&temp_json, json_str)?;

                // Write Binary (Rkyv)
                let bytes = rkyv::to_bytes::<_, 256>(self)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize binary: {}", e))?;
                std::fs::write(&temp_bin, bytes.as_slice())?;

                // Atomic Swap to prevent corruption
                std::fs::rename(temp_json, json_path)?;
                std::fs::rename(temp_bin, bin_path)?;

                Ok(())
            }
        }
    };
}
