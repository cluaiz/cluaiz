//! `cluaiz config`: the engine settings in settings.json, the same file and the same partial-update
//! rules (`OptimizationControl::merged_with`) the API and the app use.

use clap::Subcommand;
use cluaiz_core::{ActiveUserSettings, EnvironmentManager, Settings};
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;
use serde_json::{json, Value};

use crate::cli::keys;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show every setting and its value
    #[command(visible_alias = "ls")]
    List,
    /// Show one setting, e.g. `optimization.n_gpu_layers`
    Get { key: String },
    /// Change one setting; `auto` or `null` = automatic / model default
    Set { key: String, value: String },
    /// Reset one setting to its default, or every setting (settings.json is backed up first)
    Reset { key: Option<String> },
    /// Print the path of settings.json
    Path,
}

fn save(settings: ActiveUserSettings) -> Result<()> {
    Settings::update(settings).map_err(|e| eyre!("could not write settings.json: {}", e))
}

fn settings_path() -> std::path::PathBuf {
    EnvironmentManager::current().config_dir().join("settings.json")
}

pub fn execute(command: ConfigCommand) -> Result<()> {
    let current = Settings::current();
    match command {
        ConfigCommand::List => {
            let mut rows = Vec::new();
            keys::flatten(&serde_json::to_value(&current)?, "", &mut rows);
            let width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            for (key, value) in rows {
                println!("{:<width$}  {}", key, keys::display(&value), width = width);
            }
            eprintln!("{}", format!("\nnull = automatic / model default. File: {}", settings_path().display()).dimmed());
        }
        ConfigCommand::Get { key } => {
            let root = serde_json::to_value(&current)?;
            let value = keys::get(&root, &key).ok_or_else(|| unknown(&key))?;
            println!("{}", keys::display(value));
        }
        ConfigCommand::Set { key, value } => {
            let updated = set_value(&current, &key, &value)?;
            save(updated.clone())?;
            let root = serde_json::to_value(&updated)?;
            let shown = keys::get(&root, &key).map(keys::display).unwrap_or(value);
            println!("{} = {}", key, shown);
        }
        ConfigCommand::Reset { key: Some(key) } => {
            let defaults = serde_json::to_value(ActiveUserSettings::default())?;
            let default = keys::get(&defaults, &key).ok_or_else(|| unknown(&key))?.clone();
            save(apply(&current, &key, default)?)?;
            println!("{} reset to its default", key);
        }
        ConfigCommand::Reset { key: None } => {
            let path = settings_path();
            if path.exists() {
                let backup = path.with_extension("json.bak");
                std::fs::copy(&path, &backup)?;
                println!("Backed up {} to {}", path.display(), backup.display());
            }
            save(ActiveUserSettings::default())?;
            println!("All settings reset to defaults");
        }
        ConfigCommand::Path => println!("{}", settings_path().display()),
    }
    Ok(())
}

fn unknown(key: &str) -> color_eyre::Report {
    eyre!("unknown setting `{}` (see `cluaiz config list`)", key)
}

/// `raw` as typed; when that is rejected and the text is `auto`, the setting's automatic value (null).
fn set_value(settings: &ActiveUserSettings, key: &str, raw: &str) -> Result<ActiveUserSettings> {
    match apply(settings, key, keys::parse_value(raw)) {
        Err(_) if raw.eq_ignore_ascii_case("auto") => apply(settings, key, Value::Null),
        result => result,
    }
}

/// Settings with `value` at `key`. `optimization.*` and `onnx.*` go through their partial-update merge
/// (aliases like `optimization.n_ctx`, unknown keys rejected); other keys must already exist.
fn apply(settings: &ActiveUserSettings, key: &str, value: Value) -> Result<ActiveUserSettings> {
    let mut updated = settings.clone();
    match key.split_once('.') {
        Some(("optimization", rest)) => {
            let (field, nested) = match rest.split_once('.') {
                Some((field, nested)) => (field, Some(nested)),
                None => (rest, None),
            };
            let field_value = match nested {
                None => value,
                Some(nested) => {
                    let mut current = serde_json::to_value(&settings.optimization)?
                        .get(field)
                        .cloned()
                        .ok_or_else(|| unknown(key))?;
                    keys::set(&mut current, nested, value).map_err(|_| unknown(key))?;
                    current
                }
            };
            updated.optimization = settings
                .optimization
                .merged_with(&json!({ field: field_value }))
                .map_err(|e| eyre!("{}: {}", key, e))?;
        }
        Some(("onnx", field)) => {
            updated.onnx = settings.onnx.merged_with(&json!({ field: value })).map_err(|e| eyre!("{}: {}", key, e))?;
        }
        _ => {
            let mut root = serde_json::to_value(settings)?;
            keys::set(&mut root, key, value).map_err(|_| unknown(key))?;
            updated = serde_json::from_value(root).map_err(|e| eyre!("{}: {}", key, e))?;
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_validates_and_maps_auto_to_null() {
        let mut s = ActiveUserSettings::default();
        s.optimization.n_gpu_layers = Some(20);
        let s = set_value(&s, "optimization.n_gpu_layers", "auto").unwrap();
        assert_eq!(s.optimization.n_gpu_layers, None);
        let s = set_value(&s, "optimization.think_mode", "off").unwrap();
        assert_eq!(s.optimization.think_mode, cluaiz_core::FeatureState::Off);
        let s = set_value(&s, "optimization.n_ctx", "8192").unwrap();
        assert_eq!(s.optimization.custom_context_window, Some(8192));
        let s = set_value(&s, "samplers.temp", "0.6").unwrap();
        assert_eq!(s.samplers.temp, Some(0.6));
        let s = set_value(&s, "optimization.thinking_presets.low.max_tokens", "1024").unwrap();
        assert_eq!(s.optimization.thinking_presets.low.max_tokens, 1024);
        assert!(set_value(&s, "optimization.speculative_decoding", "on").is_err());
        assert!(set_value(&s, "samplers.tmp", "1").is_err());
        assert!(set_value(&s, "optimization.n_gpu_layers", "many").is_err());
    }
}
