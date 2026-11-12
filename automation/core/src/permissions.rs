//! What a flow may use (docs/automation/permissions.md): the patterns of `permissions`, read the
//! same way when a flow is saved and when each step runs.

use serde_json::{json, Map, Value};

pub const SCOPE_ORDER: [&str; 3] = ["read", "write", "exec"];

/// Methods a flow never calls: a flow does not change its own rules, mint tokens or run programs
pub const NOT_CALLABLE: [&str; 11] = [
    "flows.save",
    "flows.draft",
    "flows.fix",
    "flows.remove",
    "flows.enable",
    "flows.disable",
    "tokens.*",
    "system.exec",
    "config.set",
    "approvals.approve",
    "approvals.reject",
];

/// The Engine routes a `call` step can name (docs/automation/steps/call.md)
pub const ROUTES: [&str; 4] = ["ingest", "embeddings", "speech", "transcription"];

fn list<'a>(permissions: &'a Value, field: &str) -> impl Iterator<Item = &'a str> {
    permissions.get(field).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str)
}

/// `*` matches all; `prefix*` matches what starts with `prefix` (`mcp:github/*`, `fs.*`, `myapi/*`)
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

pub fn not_callable(method: &str) -> bool {
    NOT_CALLABLE.iter().any(|p| pattern_matches(p, method))
}

pub fn allows_tool(permissions: &Value, tool: &str) -> bool {
    list(permissions, "tools").any(|p| pattern_matches(p, tool))
}

pub fn allows_method(permissions: &Value, method: &str) -> bool {
    list(permissions, "methods").any(|p| pattern_matches(p, method))
}

/// `model: None` is the default chat model, which needs `*`
pub fn allows_model(permissions: &Value, model: Option<&str>) -> bool {
    list(permissions, "models").any(|p| match model {
        Some(model) => pattern_matches(p, model),
        None => p == "*",
    })
}

/// A scope is granted when it, or a higher one, is listed
pub fn allows_scope(permissions: &Value, scope: &str) -> bool {
    let rank = |s: &str| SCOPE_ORDER.iter().position(|o| *o == s);
    let Some(wanted) = rank(scope) else { return false };
    list(permissions, "scopes").filter_map(rank).any(|granted| granted >= wanted)
}

/// A folder includes everything below it; `~` is the home folder. Paths are compared as given
/// (the Engine checks the resolved path again when the step runs).
pub fn allows_folder(permissions: &Value, path: &str) -> bool {
    let wanted = normal(path);
    list(permissions, "folders").any(|folder| {
        let folder = normal(folder);
        wanted == folder || wanted.starts_with(&format!("{}/", folder.trim_end_matches('/')))
    })
}

fn normal(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

const LIST_FIELDS: [&str; 6] = ["tools", "methods", "folders", "network", "models", "scopes"];

/// Every item of `wanted` is allowed by `granted` (patterns included: `mcp:github/*` covers
/// `mcp:github/*` and `mcp:github/list`)
pub fn covers(granted: &Value, wanted: &Value) -> bool {
    LIST_FIELDS.iter().all(|field| {
        list(wanted, field).all(|item| match *field {
            "folders" => allows_folder(granted, item),
            "scopes" => allows_scope(granted, item),
            "models" if item == "*" => allows_model(granted, None),
            _ => list(granted, field).any(|g| pattern_matches(g, item)),
        })
    }) && (wanted.get("outside_content") != Some(&Value::Bool(true)) || granted.get("outside_content") == Some(&Value::Bool(true)))
}

/// Both lists together, each item once
pub fn merge(a: &Value, b: &Value) -> Value {
    let mut out = Map::new();
    for field in LIST_FIELDS {
        let mut items: Vec<String> = Vec::new();
        for item in list(a, field).chain(list(b, field)) {
            add(&mut items, item);
        }
        if !items.is_empty() {
            out.insert(field.into(), json!(items));
        }
    }
    if a.get("outside_content") == Some(&Value::Bool(true)) || b.get("outside_content") == Some(&Value::Bool(true)) {
        out.insert("outside_content".into(), Value::Bool(true));
    }
    Value::Object(out)
}

/// What a flow uses, collected while it is checked
#[derive(Debug, Default, Clone)]
pub struct Needed {
    pub tools: Vec<String>,
    pub methods: Vec<String>,
    pub folders: Vec<String>,
    pub models: Vec<String>,
    pub scopes: Vec<String>,
    pub outside_content: bool,
}

fn add(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|i| i == item) {
        list.push(item.to_string());
    }
}

impl Needed {
    pub fn tool(&mut self, name: &str) {
        add(&mut self.tools, name);
        self.scope("exec");
    }
    pub fn method(&mut self, name: &str) {
        add(&mut self.methods, name);
    }
    pub fn folder(&mut self, path: &str) {
        add(&mut self.folders, path);
    }
    pub fn model(&mut self, model: Option<&str>) {
        add(&mut self.models, model.unwrap_or("*"));
    }
    pub fn scope(&mut self, scope: &str) {
        if SCOPE_ORDER.contains(&scope) {
            add(&mut self.scopes, scope);
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        let mut put = |field: &str, items: &[String]| {
            if !items.is_empty() {
                map.insert(field.into(), json!(items));
            }
        };
        let mut scopes = self.scopes.clone();
        scopes.sort_by_key(|s| SCOPE_ORDER.iter().position(|o| o == s));
        put("tools", &self.tools);
        put("methods", &self.methods);
        put("folders", &self.folders);
        put("models", &self.models);
        put("scopes", &scopes);
        if self.outside_content {
            map.insert("outside_content".into(), Value::Bool(true));
        }
        Value::Object(map)
    }

    /// What the flow uses that `permissions` does not grant; `None` when all is granted
    pub fn missing(&self, permissions: &Value) -> Option<Needed> {
        let missing = Needed {
            tools: self.tools.iter().filter(|t| !allows_tool(permissions, t)).cloned().collect(),
            methods: self.methods.iter().filter(|m| !allows_method(permissions, m)).cloned().collect(),
            folders: self.folders.iter().filter(|f| !allows_folder(permissions, f)).cloned().collect(),
            models: self.models.iter().filter(|m| !allows_model(permissions, if *m == "*" { None } else { Some(m) })).cloned().collect(),
            scopes: self.scopes.iter().filter(|s| !allows_scope(permissions, s)).cloned().collect(),
            outside_content: self.outside_content && permissions.get("outside_content") != Some(&Value::Bool(true)),
        };
        let empty = missing.tools.is_empty() && missing.methods.is_empty() && missing.folders.is_empty() && missing.models.is_empty() && missing.scopes.is_empty() && !missing.outside_content;
        (!empty).then_some(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        let p = json!({ "tools": ["mcp:github/*"], "methods": ["fs.*", "notes.create"], "models": ["myapi/*"], "scopes": ["write"], "folders": ["~/Notes"] });
        assert!(allows_tool(&p, "mcp:github/create_issue"));
        assert!(!allows_tool(&p, "mcp:gitlab/x"));
        assert!(allows_method(&p, "fs.read") && allows_method(&p, "notes.create") && !allows_method(&p, "notes.delete"));
        assert!(allows_model(&p, Some("myapi/llama")) && !allows_model(&p, None));
        assert!(allows_scope(&p, "read") && allows_scope(&p, "write") && !allows_scope(&p, "exec"));
        assert!(allows_folder(&p, "~/Notes/a.md") && allows_folder(&p, "~/Notes") && !allows_folder(&p, "~/NotesOld/a"));
        assert!(not_callable("tokens.create") && not_callable("flows.save") && !not_callable("flows.run"));
    }

    #[test]
    fn covers_and_merge() {
        let granted = json!({ "tools": ["mcp:github/*"], "scopes": ["exec"], "folders": ["~/a"] });
        assert!(covers(&granted, &json!({ "tools": ["mcp:github/list", "mcp:github/*"], "scopes": ["read"], "folders": ["~/a/b"] })));
        assert!(!covers(&granted, &json!({ "tools": ["*"] })));
        assert!(!covers(&granted, &json!({ "outside_content": true })));
        assert_eq!(merge(&json!({ "tools": ["a"] }), &json!({ "tools": ["a", "b"], "models": ["*"] })), json!({ "tools": ["a", "b"], "models": ["*"] }));
    }

    #[test]
    fn missing_lists_only_what_is_not_granted() {
        let mut needed = Needed::default();
        needed.tool("mcp:github/list");
        needed.method("fs.read");
        needed.scope("read");
        let granted = json!({ "tools": ["mcp:github/*"], "scopes": ["exec"] });
        let missing = needed.missing(&granted).unwrap();
        assert_eq!(missing.to_value(), json!({ "methods": ["fs.read"] }));
        assert!(needed.missing(&json!({ "tools": ["*"], "methods": ["fs.*"], "scopes": ["exec"] })).is_none());
    }
}
