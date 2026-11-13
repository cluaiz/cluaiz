//! The settings a tool reads, keys among them, named in its `package.json` `env`:
//! `{"TAVILY_API_KEY": {"info": "...", "need": "optional"}}`.
//!
//! One rule for every tool type: a value comes from the system environment variable of that name
//! first, otherwise from the value the user saved in `config/tool_settings.json` under the tool's
//! id. A tool gets only the names its manifest lists. The rules are in cluaiz-tools
//! `docs/package-spec.md#env`.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// One setting a tool names in `env`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnvSetting {
    pub name: String,
    pub info: String,
    /// The tool cannot work without it
    pub required: bool,
}

/// The settings a manifest names, sorted by name. A name that is not upper case letters, digits
/// and `_` is ignored, so a manifest cannot name something that is not an environment variable.
pub fn declared(manifest: &Value) -> Vec<EnvSetting> {
    let Some(env) = manifest.get("env").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found: Vec<EnvSetting> = env
        .iter()
        .filter(|(name, _)| valid_name(name))
        .map(|(name, spec)| EnvSetting {
            name: name.clone(),
            info: spec.get("info").and_then(Value::as_str).unwrap_or_default().to_owned(),
            required: spec.get("need").and_then(Value::as_str) == Some("required"),
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// The value of one named setting: the system environment first, then the user's saved value
pub fn value(tool_id: &str, name: &str, config_dir: &Path) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| saved(tool_id, config_dir).remove(name))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The values of every setting the manifest names that has one
pub fn values(tool_id: &str, manifest: &Value, config_dir: &Path) -> HashMap<String, String> {
    let saved = saved(tool_id, config_dir);
    declared(manifest)
        .into_iter()
        .filter_map(|setting| {
            let value = std::env::var(&setting.name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| saved.get(&setting.name).cloned())?;
            let value = value.trim().to_owned();
            (!value.is_empty()).then_some((setting.name, value))
        })
        .collect()
}

/// Whether the system environment has a value for `name`
pub fn in_system(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
}

/// The values the user saved for a tool. The values stay out of anything returned to a caller.
pub(crate) fn saved(tool_id: &str, config_dir: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(config_dir.join("tool_settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|all| serde_json::from_value(all.get(tool_id)?.clone()).ok())
        .unwrap_or_default()
}

/// The manifest of the tool installed in `dir`
pub fn manifest_in(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_named_settings_and_the_system_comes_first() {
        let config = std::env::temp_dir().join(format!("cluaiz-tool-env-{}", std::process::id()));
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("tool_settings.json"),
            r#"{"search": {"CLUAIZ_TEST_SAVED_KEY": " saved ", "CLUAIZ_TEST_SYSTEM_KEY": "saved", "OTHER": "x"}}"#,
        )
        .unwrap();
        std::env::set_var("CLUAIZ_TEST_SYSTEM_KEY", "from-system");
        let manifest = json!({"env": {
            "CLUAIZ_TEST_SAVED_KEY": {"info": "a key", "need": "required"},
            "CLUAIZ_TEST_SYSTEM_KEY": {"info": "another"},
            "bad name": {"info": "ignored"}
        }});
        let names: Vec<String> = declared(&manifest).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["CLUAIZ_TEST_SAVED_KEY", "CLUAIZ_TEST_SYSTEM_KEY"]);
        assert!(declared(&manifest)[0].required);
        let got = values("search", &manifest, &config);
        assert_eq!(got.get("CLUAIZ_TEST_SAVED_KEY").map(String::as_str), Some("saved"));
        assert_eq!(got.get("CLUAIZ_TEST_SYSTEM_KEY").map(String::as_str), Some("from-system"));
        assert!(!got.contains_key("OTHER"), "a name the manifest does not list is never given");
        assert_eq!(value("search", "CLUAIZ_TEST_SAVED_KEY", &config).as_deref(), Some("saved"));
        std::env::remove_var("CLUAIZ_TEST_SYSTEM_KEY");
        std::fs::remove_dir_all(&config).unwrap();
    }
}
