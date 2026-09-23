use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::terminal_runner::SandboxTerminalRunner;
use crate::tools::registry::SecurityMode;

/// Declarative execution configuration deserialized from a component's `package.json`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionManifest {
    #[serde(rename = "type")]
    pub execution_type: Option<String>,
    pub scratch_dir: Option<String>,
    pub default_language: Option<String>,
    #[serde(default)]
    pub languages: HashMap<String, LanguageConfig>,
    pub command: Option<String>,
}

/// Language-specific command template and file extension
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageConfig {
    pub cmd: String,
    pub ext: String,
}

/// Dynamic, zero-hardcoding multi-language script execution engine
pub struct DeclarativeScriptRunner;

impl DeclarativeScriptRunner {
    /// Executes a dynamic code script according to the declarative manifest specifications
    pub async fn execute(
        manifest: &ExecutionManifest,
        payload_str: &str,
        cwd_path: &Path,
        sec_mode: SecurityMode,
    ) -> Result<String> {
        let parsed_payload: Value = serde_json::from_str(payload_str)
            .unwrap_or_else(|_| serde_json::json!({ "code": payload_str }));

        let default_lang = manifest.default_language.as_deref().unwrap_or("python");
        let requested_lang = parsed_payload.get("language")
            .or_else(|| parsed_payload.get("lang"))
            .and_then(|l| l.as_str())
            .unwrap_or(default_lang)
            .to_lowercase();

        let code = parsed_payload.get("code")
            .or_else(|| parsed_payload.get("script"))
            .or_else(|| parsed_payload.get("input"))
            .and_then(|c| c.as_str())
            .unwrap_or(payload_str);

        let (cmd_template, ext) = Self::resolve_language_config(manifest, &requested_lang);

        let scratch_name = manifest.scratch_dir.as_deref().unwrap_or("scratch");
        let temp_dir = cwd_path.join(scratch_name);
        let _ = std::fs::create_dir_all(&temp_dir);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let script_file = temp_dir.join(format!("eval_{}.{}", timestamp, ext));
        std::fs::write(&script_file, code)?;

        let exe_suffix = std::env::consts::EXE_SUFFIX;
        let bin_file = temp_dir.join(format!("eval_{}{}", timestamp, exe_suffix));

        let command_line = cmd_template
            .replace("{file}", &script_file.display().to_string())
            .replace("{bin}", &bin_file.display().to_string());

        tracing::info!("🚀 [ScriptRunner] Executing language '{}' via: {}", requested_lang, command_line);

        let timeout_secs = parsed_payload.get("timeout")
            .and_then(|t| t.as_u64())
            .or(Some(15));

        let run_result = SandboxTerminalRunner::execute_advanced(
            &command_line,
            cwd_path,
            sec_mode,
            None,
            None,
            timeout_secs,
            None,
        ).await?;

        // Cleanup temporary files
        let _ = std::fs::remove_file(&script_file);
        if bin_file.exists() {
            let _ = std::fs::remove_file(&bin_file);
        }

        let output_json = serde_json::to_string_pretty(&run_result)?;
        Ok(output_json)
    }

    /// Resolves command template and file extension for a given language,
    /// checking the manifest first, then falling back to the standard multi-language matrix.
    pub fn resolve_language_config(manifest: &ExecutionManifest, lang: &str) -> (String, String) {
        let normalized = lang.trim().to_lowercase();

        // 1. Check explicit manifest declaration
        if let Some(cfg) = manifest.languages.get(&normalized) {
            return (cfg.cmd.clone(), cfg.ext.clone());
        }

        // 2. Fallback to comprehensive multi-language matrix
        match normalized.as_str() {
            "python" | "py" => ("python \"{file}\"".to_string(), "py".to_string()),
            "javascript" | "js" | "node" => ("node \"{file}\"".to_string(), "js".to_string()),
            "typescript" | "ts" => ("npx ts-node \"{file}\"".to_string(), "ts".to_string()),
            "powershell" | "ps1" => ("powershell -File \"{file}\"".to_string(), "ps1".to_string()),
            "pwsh" => ("pwsh -File \"{file}\"".to_string(), "ps1".to_string()),
            "bash" | "sh" => ("bash \"{file}\"".to_string(), "sh".to_string()),
            "rust" | "rs" => ("rustc \"{file}\" -o \"{bin}\" && \"{bin}\"".to_string(), "rs".to_string()),
            "go" | "golang" => ("go run \"{file}\"".to_string(), "go".to_string()),
            "c" => ("gcc \"{file}\" -o \"{bin}\" && \"{bin}\"".to_string(), "c".to_string()),
            "cpp" | "c++" => ("g++ \"{file}\" -o \"{bin}\" && \"{bin}\"".to_string(), "cpp".to_string()),
            "java" => ("java \"{file}\"".to_string(), "java".to_string()),
            "php" => ("php \"{file}\"".to_string(), "php".to_string()),
            "ruby" | "rb" => ("ruby \"{file}\"".to_string(), "rb".to_string()),
            "lua" => ("lua \"{file}\"".to_string(), "lua".to_string()),
            "r" => ("Rscript \"{file}\"".to_string(), "r".to_string()),
            custom => {
                // Generic fallback for any arbitrary command runner
                (format!("{} \"{{file}}\"", custom), "txt".to_string())
            }
        }
    }
}
