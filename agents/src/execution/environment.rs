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

    /// The environment of a tool's program (an MCP server, a skill's script): only the system
    /// variables a program needs to run, then the tool's own values (`tool_env`: the names in its
    /// `env`, and the server's fixed `execution.env`). The user's keys and other programs' values
    /// stay out, whatever their names.
    pub fn for_tool(tool_env: &HashMap<String, String>) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = std::env::vars().filter(|(name, _)| Self::is_system_basic(name)).collect();
        env.extend(tool_env.iter().map(|(name, value)| (name.clone(), value.clone())));
        env
    }

    /// Where programs, the user's folders and temporary files are, the language and time zone, a
    /// proxy and its certificates, language toolchains, and the Engine's own address
    fn is_system_basic(name: &str) -> bool {
        const NAMES: &[&str] = &[
            // Windows
            "PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "COMSPEC", "PSMODULEPATH", "TEMP", "TMP",
            "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "PROGRAMFILES",
            "PROGRAMFILES(X86)", "PROGRAMW6432", "COMMONPROGRAMFILES", "COMMONPROGRAMFILES(X86)", "COMMONPROGRAMW6432",
            "ALLUSERSPROFILE", "PUBLIC", "USERNAME", "USERDOMAIN", "COMPUTERNAME", "OS", "PROCESSOR_ARCHITECTURE",
            "NUMBER_OF_PROCESSORS",
            // Linux and macOS
            "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LANGUAGE", "TERM", "TZ", "DISPLAY", "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
            // Network
            "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY", "ALL_PROXY", "SSL_CERT_FILE", "SSL_CERT_DIR", "NODE_EXTRA_CA_CERTS",
            "REQUESTS_CA_BUNDLE",
            // Language toolchains
            "JAVA_HOME", "GOPATH", "GOROOT", "CARGO_HOME", "RUSTUP_HOME", "NVM_HOME", "NVM_DIR", "NVM_SYMLINK",
            "VIRTUAL_ENV", "CONDA_PREFIX", "UV_CACHE_DIR", "PYTHONUTF8", "PYTHONIOENCODING",
            // The Engine's address, for a tool that calls its API
            "CLUAIZ_HOME", "CLUAIZ_PORT", "CLUAIZ_API_URL",
        ];
        let upper = name.to_ascii_uppercase();
        upper.starts_with("LC_") || NAMES.contains(&upper.as_str())
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
    fn a_tool_gets_only_the_system_basics_and_its_own_values() {
        std::env::set_var("CLUAIZ_TEST_PROVIDER_KEY", "not for tools");
        let own = HashMap::from([("WANTED".to_string(), "1".to_string())]);
        let env = EnvironmentResolver::for_tool(&own);
        assert!(!env.contains_key("CLUAIZ_TEST_PROVIDER_KEY"), "a value the tool did not name must stay out");
        assert_eq!(env.get("WANTED").map(String::as_str), Some("1"));
        assert!(env.keys().any(|name| name.eq_ignore_ascii_case("PATH")), "programs must still be found");
        std::env::remove_var("CLUAIZ_TEST_PROVIDER_KEY");
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
