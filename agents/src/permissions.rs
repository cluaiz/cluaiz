//! What a tool may reach, and the user's choice for each permission.
//!
//! A tool lists the permissions it uses in `package.json`: `permissions.<type>` with `info`,
//! `allow` and the type's own options, such as `access` and `hosts` for `network`. The Engine owns
//! the types, their options and which are sensitive. A sensitive permission is not allowed until
//! the user allows it, whatever the package says. The user's choices live in
//! `config/tool_permissions.json`, keyed by `<type>:<id>`, so an update of the tool keeps them.
//!
//! The standard is cluaiz-tools `docs/reference/permissions.md`.

use serde::Serialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The permission types the Engine knows: (type, title, its options)
pub const TYPES: &[(&str, &str, &[&str])] = &[
    ("network", "Internet", &["access", "hosts"]),
    ("files", "Files", &["can", "where"]),
    ("programs", "Programs", &["run"]),
    ("system", "System information", &["read"]),
    ("location", "Location", &["precision"]),
    ("media", "Microphone, camera, screen", &["use"]),
    ("clipboard", "Clipboard", &["can"]),
    ("notifications", "Notifications", &[]),
    ("native", "Native code", &[]),
    ("engine", "Engine methods", &["methods"]),
];

const STORE_FILE: &str = "tool_permissions.json";

/// One permission of one tool, with the user's current choice
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Permission {
    pub kind: String,
    pub title: String,
    /// What the tool does with it, from the package
    pub info: String,
    pub sensitive: bool,
    /// Allowed before the user chose
    pub default: bool,
    /// Allowed now
    pub on: bool,
    /// The user chose `on` (rather than it coming from `default`)
    pub chosen: bool,
    /// The type's options as the package wrote them, such as `{"access": "strict", "hosts": [...]}`
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub options: Map<String, Value>,
}

impl Permission {
    /// For `network` with `strict` access: the only hosts it may reach. `None` means any public host.
    pub fn strict_hosts(&self) -> Option<Vec<String>> {
        if self.kind != "network" || self.options.get("access").and_then(Value::as_str) != Some("strict") {
            return None;
        }
        Some(strings(self.options.get("hosts")))
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The title of a type, or `None` for a type the Engine does not know
pub fn kind_info(kind: &str) -> Option<&'static str> {
    TYPES.iter().find(|(k, _, _)| *k == kind).map(|(_, title, _)| *title)
}

/// Whether the Engine treats this permission as sensitive. The rules are in
/// `docs/reference/permissions.md`; an option the Engine cannot read counts as sensitive.
pub fn is_sensitive(kind: &str, options: &Map<String, Value>) -> bool {
    let list = |name: &str| strings(options.get(name));
    match kind {
        "programs" | "location" | "media" | "native" => true,
        "files" => {
            let can = list("can");
            can.is_empty() || can.iter().any(|action| action != "read") || options.get("where").and_then(Value::as_str) != Some("workspace")
        }
        "system" => list("read").iter().any(|fact| fact == "hostname" || fact == "username"),
        "clipboard" => list("can").iter().any(|action| action == "read"),
        "network" | "notifications" | "engine" => false,
        _ => true,
    }
}

/// The permissions a manifest declares, before the user's choices. Unknown types are ignored.
pub fn declared(manifest: &Value) -> Vec<Permission> {
    let Some(perms) = manifest.get("permissions").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (kind, title, option_names) in TYPES {
        let Some(spec) = perms.get(*kind).and_then(Value::as_object) else {
            continue;
        };
        let options: Map<String, Value> =
            spec.iter().filter(|(name, _)| option_names.contains(&name.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        let sensitive = is_sensitive(kind, &options);
        let default = !sensitive && spec.get("allow").and_then(Value::as_bool).unwrap_or(true);
        found.push(Permission {
            kind: (*kind).to_owned(),
            title: (*title).to_owned(),
            info: spec.get("info").and_then(Value::as_str).unwrap_or_default().to_owned(),
            sensitive,
            default,
            on: default,
            chosen: false,
            options,
        });
    }
    found
}

/// The key of a tool in the store: `plugin:time`
pub fn tool_key(tool_type: &str, id: &str) -> String {
    format!("{tool_type}:{id}")
}

fn store_path(config_dir: &Path) -> PathBuf {
    config_dir.join(STORE_FILE)
}

fn read_store(config_dir: &Path) -> Map<String, Value> {
    std::fs::read_to_string(store_path(config_dir))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

/// The user's choices for one tool: type -> on
pub fn choices(config_dir: &Path, tool_type: &str, id: &str) -> Map<String, Value> {
    read_store(config_dir)
        .get(&tool_key(tool_type, id))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// The tool's permissions with the user's choices applied
pub fn effective(config_dir: &Path, tool_type: &str, id: &str, manifest: &Value) -> Vec<Permission> {
    let chosen = choices(config_dir, tool_type, id);
    declared(manifest)
        .into_iter()
        .map(|mut permission| {
            if let Some(on) = chosen.get(&permission.kind).and_then(Value::as_bool) {
                permission.on = on;
                permission.chosen = true;
            }
            permission
        })
        .collect()
}

/// Whether `kind` is allowed for the tool. A type the tool does not declare is not.
pub fn is_on(config_dir: &Path, tool_type: &str, id: &str, manifest: &Value, kind: &str) -> bool {
    effective(config_dir, tool_type, id, manifest).into_iter().any(|p| p.kind == kind && p.on)
}

/// Records the user's choice. `kind` must be a known type.
pub fn set(config_dir: &Path, tool_type: &str, id: &str, kind: &str, on: bool) -> anyhow::Result<()> {
    if kind_info(kind).is_none() {
        let known: Vec<&str> = TYPES.iter().map(|(k, _, _)| *k).collect();
        anyhow::bail!("'{kind}' is not a permission. Permissions: {}", known.join(", "));
    }
    let mut store = read_store(config_dir);
    let entry = store.entry(tool_key(tool_type, id)).or_insert_with(|| Value::Object(Map::new()));
    if let Some(map) = entry.as_object_mut() {
        map.insert(kind.to_owned(), Value::Bool(on));
    }
    std::fs::create_dir_all(config_dir)?;
    let text = serde_json::to_string_pretty(&Value::Object(store))?;
    let path = store_path(config_dir);
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &path)?;
    Ok(())
}

/// Removes the user's choices for a tool (when it is removed)
pub fn forget(config_dir: &Path, tool_type: &str, id: &str) {
    let mut store = read_store(config_dir);
    if store.remove(&tool_key(tool_type, id)).is_some() {
        if let Ok(text) = serde_json::to_string_pretty(&Value::Object(store)) {
            let _ = std::fs::write(store_path(config_dir), text);
        }
    }
}

/// The message a tool's call gets when a permission it needs is refused
pub fn off_message(tool_type: &str, id: &str, kind: &str) -> String {
    let title = kind_info(kind).unwrap_or(kind);
    format!("{title} is not allowed for {tool_type} '{id}'. Allow it with: cluaiz {tool_type} permission {id} {kind} on")
}

/// The approval tags of a manifest's permissions, which `requires_approval` reads: `net:fetch`,
/// `fs:read`, `fs:write`, `exec:programs`, or the type's own name.
pub fn approval_tags(manifest: &Value) -> Vec<String> {
    declared(manifest)
        .into_iter()
        .map(|p| match p.kind.as_str() {
            "network" => "net:fetch".to_owned(),
            "files" if strings(p.options.get("can")).iter().all(|action| action == "read") => "fs:read".to_owned(),
            "files" => "fs:write".to_owned(),
            "programs" | "native" => "exec:programs".to_owned(),
            other => other.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-perm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn types_options_and_sensitivity() {
        let manifest = json!({"permissions": {
            "network": {"info": "holidays", "allow": true, "access": "strict", "hosts": ["date.nager.at"]},
            "files": {"info": "reads the report", "can": ["read"], "where": "workspace"},
            "programs": {"info": "git", "allow": true, "run": ["git"]},
            "location": {"info": "your city", "allow": true, "precision": "approximate"},
            "clock": {"info": "removed"},
            "camera": {"info": "unknown type"}
        }});
        let got = declared(&manifest);
        let find = |kind: &str| got.iter().find(|p| p.kind == kind).cloned();
        let network = find("network").unwrap();
        assert!(network.on && !network.sensitive);
        assert_eq!(network.strict_hosts(), Some(vec!["date.nager.at".to_string()]));
        let files = find("files").unwrap();
        assert!(files.on && !files.sensitive, "reading the workspace is not sensitive");
        assert!(!find("programs").unwrap().on, "a sensitive type is not allowed whatever the package says");
        assert!(!find("location").unwrap().on);
        assert!(find("clock").is_none() && find("camera").is_none());
        assert!(declared(&json!({"permissions": {"network_access": true, "files_read": {"why": "x"}}})).is_empty());
    }

    #[test]
    fn sensitivity_follows_the_options() {
        let options = |value: Value| value.as_object().cloned().unwrap();
        assert!(is_sensitive("files", &options(json!({"can": ["read", "update"], "where": "workspace"}))));
        assert!(is_sensitive("files", &options(json!({"can": ["read"], "where": "home"}))));
        assert!(is_sensitive("files", &options(json!({}))), "options it cannot read count as sensitive");
        assert!(!is_sensitive("system", &options(json!({"read": ["cpu", "memory"]}))));
        assert!(is_sensitive("system", &options(json!({"read": ["username"]}))));
        assert!(is_sensitive("clipboard", &options(json!({"can": ["read"]}))));
        assert!(!is_sensitive("clipboard", &options(json!({"can": ["write"]}))));
        let full = declared(&json!({"permissions": {"network": {"info": "x", "access": "full"}}}));
        assert_eq!(full[0].strict_hosts(), None, "full access reaches any public host");
    }

    #[test]
    fn approval_tags_name_what_is_reached() {
        let manifest = json!({"permissions": {
            "network": {"info": "x", "access": "full"},
            "files": {"info": "x", "can": ["read"], "where": "workspace"},
            "programs": {"info": "x", "run": ["git"]}
        }});
        assert_eq!(approval_tags(&manifest), ["net:fetch", "fs:read", "exec:programs"]);
        let writes = json!({"permissions": {"files": {"info": "x", "can": ["create"], "where": "workspace"}}});
        assert_eq!(approval_tags(&writes), ["fs:write"]);
    }

    #[test]
    fn choices_override_defaults_and_survive() {
        let dir = temp("choices");
        let manifest = json!({"permissions": {"network": {"info": "x", "access": "full"}}});
        assert!(is_on(&dir, "plugin", "time", &manifest, "network"));
        assert!(!is_on(&dir, "plugin", "time", &manifest, "location"), "an undeclared type is not allowed");
        set(&dir, "plugin", "time", "network", false).unwrap();
        assert!(!is_on(&dir, "plugin", "time", &manifest, "network"));
        assert!(is_on(&dir, "plugin", "other", &manifest, "network"), "choices are per tool");
        assert!(set(&dir, "plugin", "time", "clock", false).is_err(), "clock is not a permission");
        forget(&dir, "plugin", "time");
        assert!(is_on(&dir, "plugin", "time", &manifest, "network"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
