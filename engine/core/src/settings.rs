//! ⚙️ Centralized Settings & User Profiles for Cluaiz Engine
//! Single Source of Truth for system configuration, hardware preferences, and optimization switches.

use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use once_cell::sync::Lazy;
use crate::env::EnvironmentManager;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum FeatureState {
    On,
    Off,
    #[default]
    Auto,
}

impl FeatureState {
    pub fn is_active(&self) -> bool {
        matches!(self, FeatureState::On | FeatureState::Auto)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum KvCacheQuantization {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto,
    #[serde(rename = "Kv16", alias = "kv16")]
    Kv16,
    #[serde(rename = "Kv8", alias = "kv8")]
    Kv8,
    #[serde(rename = "Kv4", alias = "kv4")]
    Kv4,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ContextShiftingMode {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto,
    #[serde(rename = "Off", alias = "off")]
    Off,
    #[serde(rename = "Minimal", alias = "minimal")]
    Minimal,
    #[serde(rename = "Standard", alias = "standard", alias = "on", alias = "On")]
    Standard,
    #[serde(rename = "Aggressive", alias = "aggressive")]
    Aggressive,
    #[serde(rename = "Extreme", alias = "extreme")]
    Extreme,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizationControl {
    #[serde(default)]
    pub flash_attention: FeatureState,
    #[serde(default)]
    pub kv_cache_quantization: KvCacheQuantization,
    #[serde(default, alias = "force_vram_reclaim")]
    pub hybrid_memory: FeatureState,
    #[serde(default)]
    pub force_memory_lock: FeatureState,
    #[serde(default)]
    pub context_shifting: ContextShiftingMode,
    #[serde(default)]
    pub speculative_decoding: FeatureState,
    #[serde(default)]
    pub draft_model_path: Option<String>,
    #[serde(default)]
    pub extreme_moe_streaming: FeatureState,
    /// Direct GB safety buffer override for VRAM. None = dynamic auto mode.
    #[serde(default)]
    pub custom_vram_buffer_gb: Option<f64>,
    /// Direct GB safety buffer override for CPU RAM. None = dynamic auto mode.
    #[serde(default)]
    pub custom_ram_buffer_gb: Option<f64>,
}

impl Default for OptimizationControl {
    fn default() -> Self {
        Self {
            flash_attention: FeatureState::Auto,
            kv_cache_quantization: KvCacheQuantization::Auto,
            hybrid_memory: FeatureState::Auto,
            force_memory_lock: FeatureState::Auto,
            context_shifting: ContextShiftingMode::Auto,
            speculative_decoding: FeatureState::Off,
            draft_model_path: None,
            extreme_moe_streaming: FeatureState::Auto,
            custom_vram_buffer_gb: None,
            custom_ram_buffer_gb: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveUserSettings {
    #[serde(default)]
    pub optimization: OptimizationControl,
    #[serde(default)]
    pub hardware_preset: String,
    #[serde(default)]
    pub telemetry_enabled: bool,
}

impl Default for ActiveUserSettings {
    fn default() -> Self {
        Self {
            optimization: OptimizationControl::default(),
            hardware_preset: "Balanced".to_string(),
            telemetry_enabled: true,
        }
    }
}

static CACHED_SETTINGS: Lazy<RwLock<ActiveUserSettings>> = Lazy::new(|| {
    let loaded = Settings::load_from_disk().unwrap_or_default();
    RwLock::new(loaded)
});

pub struct Settings;

impl Settings {
    /// Returns a copy of the current cached active user settings (0 disk reads).
    pub fn current() -> ActiveUserSettings {
        CACHED_SETTINGS.read().unwrap().clone()
    }

    /// Updates the cached settings and persists them to ~/.cluaiz/engine/config/settings.json
    pub fn update(new_settings: ActiveUserSettings) -> anyhow::Result<()> {
        let env = EnvironmentManager::current();
        env.ensure_config_dir()?;
        let path = env.config_dir().join("settings.json");
        let content = serde_json::to_string_pretty(&new_settings)?;
        std::fs::write(&path, content)?;
        *CACHED_SETTINGS.write().unwrap() = new_settings;
        Ok(())
    }

    /// Loads settings directly from disk or creates default if missing.
    pub fn load_from_disk() -> anyhow::Result<ActiveUserSettings> {
        let env = EnvironmentManager::current();
        let path = env.config_dir().join("settings.json");
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            let settings: ActiveUserSettings = serde_json::from_str(&data)?;
            Ok(settings)
        } else {
            Ok(ActiveUserSettings::default())
        }
    }
}
