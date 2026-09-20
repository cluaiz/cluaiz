use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Environment configuration prepared for child process execution
#[derive(Debug, Clone)]
pub struct ResolvedEnvironment {
    pub cwd: PathBuf,
    pub env_vars: HashMap<String, String>,
}

/// Resolves paths, virtual environments, and applies security scrubbing to environment variables
pub struct EnvironmentResolver;

impl EnvironmentResolver {
    /// List of sensitive environment variables to scrub (Hermes standard)
    pub const SENSITIVE_VARS: &'static [&'static str] = &[
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DEEPSEEK_API_KEY",
        "GEMINI_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "GITLAB_TOKEN",
        "STRIPE_API_KEY",
        "DATABASE_URL",
        "POSTGRES_PASSWORD",
        "REDIS_PASSWORD",
        "SECRET_KEY",
    ];

    /// Prepares a resolved environment for a command execution
    pub fn resolve(
        cwd: &Path,
        venv_dir: Option<&Path>,
        scrub_secrets: bool,
    ) -> ResolvedEnvironment {
        let mut env_map = HashMap::new();

        // 1. Inherit existing host environment
        for (key, val) in std::env::vars() {
            let key_upper = key.to_uppercase();

            // Check if variable should be scrubbed
            if scrub_secrets && Self::is_sensitive(&key_upper) {
                continue;
            }

            env_map.insert(key, val);
        }

        // 2. Inject Virtual Environment (PATH & VIRTUAL_ENV) if present
        if let Some(venv) = venv_dir {
            #[cfg(target_os = "windows")]
            let venv_bin = venv.join("Scripts");
            #[cfg(not(target_os = "windows"))]
            let venv_bin = venv.join("bin");

            if venv_bin.exists() {
                #[cfg(target_os = "windows")]
                let separator = ";";
                #[cfg(not(target_os = "windows"))]
                let separator = ":";

                let current_path = env_map.get("PATH")
                    .cloned()
                    .or_else(|| env_map.get("Path").cloned())
                    .unwrap_or_default();

                let new_path = format!("{}{}{}", venv_bin.display(), separator, current_path);
                env_map.insert("PATH".to_string(), new_path);
                env_map.insert("VIRTUAL_ENV".to_string(), venv.display().to_string());
            }
        }

        ResolvedEnvironment {
            cwd: cwd.to_path_buf(),
            env_vars: env_map,
        }
    }

    /// Checks if a variable name matches known sensitive secret patterns
    fn is_sensitive(var_name_upper: &str) -> bool {
        Self::SENSITIVE_VARS.iter().any(|&s| s == var_name_upper)
            || var_name_upper.contains("API_KEY")
            || var_name_upper.contains("SECRET")
            || var_name_upper.contains("TOKEN")
            || var_name_upper.contains("PASSWORD")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_scrubbing() {
        std::env::set_var("CLUAIZ_TEST_API_KEY", "secret_12345");
        let cwd = std::env::temp_dir();

        let resolved = EnvironmentResolver::resolve(&cwd, None, true);
        assert!(!resolved.env_vars.contains_key("CLUAIZ_TEST_API_KEY"), "Sensitive API key must be scrubbed");

        std::env::remove_var("CLUAIZ_TEST_API_KEY");
    }

    #[test]
    fn test_venv_path_injection() {
        let temp_dir = std::env::temp_dir();
        let fake_venv = temp_dir.join("cluaiz_test_venv");
        #[cfg(target_os = "windows")]
        let bin_dir = fake_venv.join("Scripts");
        #[cfg(not(target_os = "windows"))]
        let bin_dir = fake_venv.join("bin");

        let _ = std::fs::create_dir_all(&bin_dir);

        let resolved = EnvironmentResolver::resolve(&temp_dir, Some(&fake_venv), false);
        assert_eq!(resolved.env_vars.get("VIRTUAL_ENV").unwrap(), &fake_venv.display().to_string());
        let path = resolved.env_vars.get("PATH").unwrap();
        assert!(path.starts_with(&bin_dir.display().to_string()), "PATH must prioritize venv binary directory");

        let _ = std::fs::remove_dir_all(&fake_venv);
    }
}
