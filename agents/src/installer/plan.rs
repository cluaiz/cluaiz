//! What an install will do, before anything is downloaded: the tool, the tools it depends on,
//! and the permissions each one asks for, each marked as done without asking, asked, or only
//! suggested. The user's mode (`full_access`, `sandboxed`, `strict`) decides which.
//!
//! The rules are in cluaiz-tools `docs/package-spec.md` (Dependencies, Permissions).

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

use crate::permissions::{self, Permission};

/// How much a tool depends on another (`dependencies.<kind>.<id>.need`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    Required,
    Recommended,
    Optional,
}

impl Need {
    /// `need` from a manifest; absent or unknown counts as required, as before `need` existed
    pub fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("recommended") => Need::Recommended,
            Some("optional") => Need::Optional,
            _ => Need::Required,
        }
    }
}

/// What happens to one tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// Installed without asking
    Install,
    /// Installed only if the user says yes
    Ask,
    /// Named to the user, never installed by this install
    Suggest,
    /// Already installed; kept as it is
    Installed,
}

/// What happens to one permission of a tool that is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    /// On by its default, without asking
    On,
    /// Off by its default, without asking
    Off,
    /// On only if the user says yes
    Ask,
}

/// The user's mode as a known value; anything else counts as `sandboxed`
pub fn mode_of(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "full_access" => "full_access",
        "strict" => "strict",
        _ => "sandboxed",
    }
}

/// The table "Installing by the user's mode": a dependency that asks for a sensitive permission
/// is never installed without asking in `sandboxed`
pub fn decide_tool(need: Need, mode: &str, wants_sensitive: bool) -> Choice {
    match (need, mode_of(mode)) {
        (Need::Optional, _) => Choice::Suggest,
        (_, "full_access") => Choice::Install,
        (Need::Required, "sandboxed") if !wants_sensitive => Choice::Install,
        _ => Choice::Ask,
    }
}

/// A sensitive permission is always asked for; in `strict` every permission is
pub fn decide_permission(permission: &Permission, mode: &str) -> PermissionChoice {
    if permission.sensitive || mode_of(mode) == "strict" {
        PermissionChoice::Ask
    } else if permission.default {
        PermissionChoice::On
    } else {
        PermissionChoice::Off
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedTool {
    /// `skill`, `plugin` or `mcp`
    pub kind: String,
    pub id: String,
    pub need: Need,
    /// One sentence from the package: what the dependency is for
    pub info: String,
    /// The tool that lists it (`skill:deep-research`); empty for the tool asked for
    pub needed_by: String,
    pub choice: Choice,
}

impl PlannedTool {
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind, self.id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedPermission {
    /// The tool's `kind:id`
    pub tool: String,
    #[serde(flatten)]
    pub permission: Permission,
    pub choice: PermissionChoice,
}

impl PlannedPermission {
    /// `plugin:time:network`
    pub fn key(&self) -> String {
        format!("{}:{}", self.tool, self.permission.kind)
    }
}

/// Everything one install will do. `tools` is in install order: dependencies first, the tool
/// asked for last.
#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub mode: String,
    pub tools: Vec<PlannedTool>,
    pub permissions: Vec<PlannedPermission>,
}

/// What the user said yes to: tool keys (`plugin:web-search`) and permission keys
/// (`plugin:time:location`)
#[derive(Debug, Clone, Default)]
pub struct Approved {
    pub tools: HashSet<String>,
    pub permissions: HashSet<String>,
}

impl Approved {
    /// Yes to every question
    pub fn all(plan: &InstallPlan) -> Self {
        Self {
            tools: plan.tools.iter().filter(|t| t.choice == Choice::Ask).map(PlannedTool::key).collect(),
            permissions: plan.permissions.iter().filter(|p| p.choice == PermissionChoice::Ask).map(PlannedPermission::key).collect(),
        }
    }

    /// An answer given as JSON: `{"tools": ["plugin:web-search"], "permissions": ["plugin:time:location"]}`
    pub fn from_json(value: &Value) -> Option<Self> {
        let list = |key: &str| -> Option<HashSet<String>> {
            Some(value.get(key)?.as_array()?.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        };
        let tools = list("tools");
        let permissions = list("permissions");
        (tools.is_some() || permissions.is_some()).then(|| Self { tools: tools.unwrap_or_default(), permissions: permissions.unwrap_or_default() })
    }
}

/// The plan after the user's answers
#[derive(Debug, Clone, Default, Serialize)]
pub struct Resolved {
    /// `(kind, id)` to install, in order
    pub install: Vec<(String, String)>,
    /// `(kind, id, permission, on)` to record as the user's choice
    pub permissions: Vec<(String, String, String, bool)>,
    /// Asked about and not approved
    pub skipped: Vec<String>,
    /// Optional tools that could help
    pub suggested: Vec<String>,
}

impl InstallPlan {
    /// Whether anything needs an answer
    pub fn has_questions(&self) -> bool {
        self.tools.iter().any(|t| t.choice == Choice::Ask) || self.permissions.iter().any(|p| p.choice == PermissionChoice::Ask)
    }

    /// Applies the answers. A required tool that was asked about and not approved stops the
    /// whole install: `Err` says what to run instead.
    pub fn resolve(&self, approved: &Approved) -> Result<Resolved, String> {
        let mut resolved = Resolved::default();
        let mut refused_required = Vec::new();
        let mut left_out: HashSet<String> = HashSet::new();
        for tool in &self.tools {
            let key = tool.key();
            match tool.choice {
                Choice::Installed => {}
                Choice::Suggest => {
                    resolved.suggested.push(format!("{} {} ({}): cluaiz {} install {}", tool.kind, tool.id, tool.info, tool.kind, tool.id));
                    left_out.insert(key);
                }
                Choice::Ask if !approved.tools.contains(&key) => {
                    if tool.need == Need::Required {
                        refused_required.push(key.clone());
                    } else {
                        resolved.skipped.push(format!("{} {} ({}): cluaiz {} install {}", tool.kind, tool.id, tool.info, tool.kind, tool.id));
                    }
                    left_out.insert(key);
                }
                Choice::Install | Choice::Ask => resolved.install.push((tool.kind.clone(), tool.id.clone())),
            }
        }
        if !refused_required.is_empty() {
            let root = self.tools.last().map(PlannedTool::key).unwrap_or_default();
            return Err(format!(
                "{root} needs {}, which was not approved, so nothing was installed. Approve it, or run the install with --yes.",
                refused_required.join(", ")
            ));
        }
        for planned in &self.permissions {
            if left_out.contains(&planned.tool) {
                continue;
            }
            if planned.choice == PermissionChoice::Ask {
                let (kind, id) = planned.tool.split_once(':').unwrap_or(("", &planned.tool));
                resolved.permissions.push((kind.to_owned(), id.to_owned(), planned.permission.kind.clone(), approved.permissions.contains(&planned.key())));
            }
        }
        Ok(resolved)
    }

    /// Lines for a person: what will be installed, what is asked, what is only suggested
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for tool in &self.tools {
            let what = match tool.choice {
                Choice::Install => "install",
                Choice::Ask => "ask",
                Choice::Suggest => "suggested",
                Choice::Installed => "already installed",
            };
            let need = if tool.needed_by.is_empty() {
                "asked for".to_owned()
            } else {
                format!("{:?} by {}", tool.need, tool.needed_by).to_lowercase()
            };
            let why = if tool.info.is_empty() { String::new() } else { format!(": {}", tool.info) };
            lines.push(format!("{} {} [{}, {}]{}", tool.kind, tool.id, need, what, why));
        }
        for planned in &self.permissions {
            let what = match planned.choice {
                PermissionChoice::On => "on",
                PermissionChoice::Off => "off",
                PermissionChoice::Ask => "ask",
            };
            let sensitive = if planned.permission.sensitive { ", sensitive" } else { "" };
            lines.push(format!(
                "permission {} for {} [{}{}]: {}",
                planned.permission.title, planned.tool, what, sensitive, planned.permission.info
            ));
        }
        lines
    }
}

/// The dependencies a manifest lists: (`skill` | `plugin` | `mcp`, id, need, info)
pub fn dependencies(manifest: &Value) -> Vec<(&'static str, String, Need, String)> {
    let mut found = Vec::new();
    for (key, kind) in [("plugins", "plugin"), ("mcp", "mcp"), ("skills", "skill")] {
        if let Some(entries) = manifest["dependencies"][key].as_object() {
            for (id, spec) in entries {
                if !super::ToolsInstaller::is_valid_component_id(id) {
                    tracing::warn!("⚠️ [ToolsEngine] Ignoring dependency with an invalid name '{}'", id);
                    continue;
                }
                let need = Need::parse(spec.get("need").and_then(Value::as_str));
                let info = spec.get("info").and_then(Value::as_str).unwrap_or_default().to_owned();
                found.push((kind, id.clone(), need, info));
            }
        }
    }
    found
}

/// The planned permissions of one tool that is going to be installed
pub fn plan_permissions(tool: &str, manifest: &Value, mode: &str) -> Vec<PlannedPermission> {
    permissions::declared(manifest)
        .into_iter()
        .map(|permission| {
            let choice = decide_permission(&permission, mode);
            PlannedPermission { tool: tool.to_owned(), permission, choice }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_mode_table() {
        use Choice::*;
        use Need::*;
        assert_eq!(decide_tool(Required, "full_access", true), Install);
        assert_eq!(decide_tool(Recommended, "full_access", false), Install);
        assert_eq!(decide_tool(Required, "sandboxed", false), Install);
        assert_eq!(decide_tool(Required, "sandboxed", true), Ask);
        assert_eq!(decide_tool(Recommended, "sandboxed", false), Ask);
        assert_eq!(decide_tool(Required, "strict", false), Ask);
        assert_eq!(decide_tool(Optional, "full_access", false), Suggest);
        assert_eq!(decide_tool(Required, "something else", false), Install, "an unknown mode counts as sandboxed");
    }

    #[test]
    fn permissions_by_mode() {
        let manifest = json!({"permissions": {
            "network": {"info": "x", "access": "full"},
            "location": {"info": "y", "precision": "approximate"},
            "notifications": {"info": "z", "allow": false}
        }});
        let full = plan_permissions("plugin:time", &manifest, "full_access");
        let choice = |plan: &[PlannedPermission], kind: &str| plan.iter().find(|p| p.permission.kind == kind).unwrap().choice;
        assert_eq!(choice(&full, "network"), PermissionChoice::On);
        assert_eq!(choice(&full, "location"), PermissionChoice::Ask, "sensitive is asked in every mode");
        assert_eq!(choice(&full, "notifications"), PermissionChoice::Off);
        let strict = plan_permissions("plugin:time", &manifest, "strict");
        assert_eq!(choice(&strict, "network"), PermissionChoice::Ask);
    }

    fn plan() -> InstallPlan {
        let tool = |kind: &str, id: &str, need, by: &str, choice| PlannedTool {
            kind: kind.into(), id: id.into(), need, info: "w".into(), needed_by: by.into(), choice,
        };
        let manifest = json!({"permissions": {"location": {"info": "city", "precision": "approximate"}, "network": {"info": "net", "access": "full"}}});
        InstallPlan {
            mode: "sandboxed".into(),
            tools: vec![
                tool("plugin", "web-search", Need::Required, "skill:research", Choice::Install),
                tool("mcp", "browser", Need::Recommended, "skill:research", Choice::Ask),
                tool("mcp", "semgrep", Need::Optional, "skill:research", Choice::Suggest),
                tool("skill", "research", Need::Required, "", Choice::Install),
            ],
            permissions: plan_permissions("skill:research", &manifest, "sandboxed"),
        }
    }

    #[test]
    fn answers_decide_what_is_installed() {
        let plan = plan();
        assert!(plan.has_questions());
        let none = plan.resolve(&Approved::default()).unwrap();
        assert_eq!(none.install, [("plugin".to_string(), "web-search".to_string()), ("skill".to_string(), "research".to_string())]);
        assert_eq!(none.skipped.len(), 1);
        assert_eq!(none.suggested.len(), 1);
        assert_eq!(none.permissions, [("skill".to_string(), "research".to_string(), "location".to_string(), false)]);
        let all = plan.resolve(&Approved::all(&plan)).unwrap();
        assert_eq!(all.install.len(), 3);
        assert!(all.permissions[0].3, "approved sensitive permission is on");
        let picked = Approved::from_json(&json!({"tools": ["mcp:browser"]})).unwrap();
        assert!(!plan.resolve(&picked).unwrap().permissions[0].3);
    }

    #[test]
    fn a_refused_required_tool_installs_nothing() {
        let mut plan = plan();
        plan.tools[0].choice = Choice::Ask;
        let error = plan.resolve(&Approved::default()).unwrap_err();
        assert!(error.contains("plugin:web-search") && error.contains("nothing was installed"), "{error}");
    }

    #[test]
    fn dependencies_are_read_with_need_and_info() {
        let manifest = json!({"dependencies": {
            "plugins": {"web-search": {"version": "^1.0.0", "url": "u", "need": "recommended", "info": "search"}},
            "mcp": {"browser": {"version": "^1.0.0", "url": "u"}, "../bad": {}},
            "extensions": {"x": {}}
        }});
        let got = dependencies(&manifest);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("plugin", "web-search".to_string(), Need::Recommended, "search".to_string()));
        assert_eq!(got[1].2, Need::Required, "no need counts as required");
    }
}
