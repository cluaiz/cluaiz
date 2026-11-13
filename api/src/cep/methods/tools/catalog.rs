use super::common::{self, Kind, Target};
use crate::cep::{roots, CallCtx};
use cluaiz_agents::{ExecutionMode, SecurityMode, ToolsEngine};
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct List {
    /// Only tools of this kind; all when absent
    pub kind: Option<Kind>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Discover {
    /// A user message, as the chat would see it
    pub query: String,
    /// The message before it (follow-ups such as "and in Hindi?" use it)
    pub previous: Option<String>,
    /// Also answer `evidence`: each matching tool's score, matched words and where they matched
    #[serde(default)]
    pub explain: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// false: the tool is not offered to the model (an MCP server is also stopped)
    pub enabled: Option<bool>,
    /// inherit, require_approval or always_allow
    pub security_mode: Option<String>,
    /// auto, manual or confirm_destructive
    pub execution_mode: Option<String>,
    /// Trust the MCP server's own read-only / destructive hints
    pub trust_annotations: Option<bool>,
    /// Values the tool asks the user for, by name (see `user_settings` in the result), such as an
    /// API key. An empty string removes one. They are saved in the user's settings, never in the
    /// tool's files, and never returned
    pub user_settings: Option<std::collections::BTreeMap<String, String>>,
    /// Switch the tool's permissions on or off by kind (see `permissions` in the result), such as
    /// `{"network": false}`. Only types the tool asks for
    pub permissions: Option<std::collections::BTreeMap<String, bool>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Set {
    /// skill, plugin or mcp
    pub kind: Kind,
    /// The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens
    pub name: String,
    /// Only the fields to change
    pub patch: Patch,
}

pub async fn list(ctx: CallCtx, call: Call, params: List) -> Result<Value, CepError> {
    common::work(ctx, call, move |_, _| {
        let mut entries = ToolsEngine::list_all_tools().map_err(common::unavailable)?;
        entries.retain(|entry| {
            params
                .kind
                .is_none_or(|kind| entry.category == kind.as_str())
        });
        entries.sort_by(|a, b| (&a.category, &a.id).cmp(&(&b.category, &b.id)));
        let entries: Result<Vec<_>, CepError> = entries
            .into_iter()
            .map(|entry| {
                let path = std::path::Path::new(&entry.local_dir);
                let manifest = std::fs::read_to_string(path.join("package.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok());
                let functions = cluaiz_agents::tool_search::listed_functions(
                    &entry.category,
                    path,
                    manifest.as_ref(),
                );
                let mut value = serde_json::to_value(entry).map_err(common::unavailable)?;
                value["functions"] = json!(functions);
                Ok(value)
            })
            .collect();
        Ok(json!(entries?))
    })
    .await
}

pub async fn discover(ctx: CallCtx, call: Call, params: Discover) -> Result<Value, CepError> {
    common::work(ctx, call, move |_, _| {
        let catalog = cluaiz_agents::ToolSearch::catalog(&[]);
        let candidates = cluaiz_agents::ToolSearch::suggest(&catalog, &params.query, params.previous.as_deref());
        let mut result = json!({"query":params.query,
            "actionable":cluaiz_agents::tool_search::is_actionable(&params.query),
            "candidates":cluaiz_agents::ToolSearch::candidates_json(&catalog, &candidates),"installed":catalog.len()});
        if params.explain {
            result["evidence"] = cluaiz_agents::ToolSearch::evidence(&catalog, &params.query, params.previous.as_deref());
        }
        Ok(result)
    }).await
}

/// The values the tool asks the user for: name, whether it is a key, and whether it is set. Never
/// the values themselves.
fn user_settings(target: &Target) -> Vec<cluaiz_agents::plugins::net::UserSetting> {
    let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
    ToolsEngine::resolve_component_dir(target.kind.as_str(), &target.name)
        .map(|dir| cluaiz_agents::plugins::net::user_settings(target.kind.as_str(), &dir, &config_dir))
        .unwrap_or_default()
}

/// The tool's `package.json`, or null
fn manifest(entry: &cluaiz_agents::ToolEntry) -> Value {
    std::fs::read_to_string(std::path::Path::new(&entry.local_dir).join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn settings(entry: &cluaiz_agents::ToolEntry, target: &Target) -> Value {
    let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
    let permissions = cluaiz_agents::permissions::effective(&config_dir, target.kind.as_str(), &target.name, &manifest(entry));
    json!({"schema":{},"values":{"enabled":entry.enabled,"security_mode":entry.security_mode,
        "execution_mode":entry.execution_mode,"trust_annotations":entry.trust_annotations},
        "user_settings":user_settings(target),"permissions":permissions})
}

pub async fn get(ctx: CallCtx, call: Call, target: Target) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        common::directory(&call, &workspace, &target)?;
        Ok(settings(&common::entry(&target)?, &target))
    })
    .await
}

pub async fn set(ctx: CallCtx, call: Call, params: Set) -> Result<Value, CepError> {
    let target = Target {
        kind: params.kind,
        name: params.name,
    };
    let security = params
        .patch
        .security_mode
        .as_deref()
        .map(|value| {
            SecurityMode::from_raw(value)
                .ok_or_else(|| roots::invalid("patch.security_mode", "Unknown tool security mode"))
        })
        .transpose()?;
    let execution = params
        .patch
        .execution_mode
        .as_deref()
        .map(|value| {
            ExecutionMode::from_raw(value).ok_or_else(|| {
                roots::invalid("patch.execution_mode", "Unknown tool execution mode")
            })
        })
        .transpose()?;
    // A server reads its environment when it starts: new values reach it from its next start
    let stop = target.kind == Kind::Mcp
        && (params.patch.enabled == Some(false) || params.patch.user_settings.as_ref().is_some_and(|values| !values.is_empty()));
    let name = target.name.clone();
    let value = common::work(ctx, call, move |call, workspace| {
        common::directory(&call, &workspace, &target)?;
        let mut registry = ToolsEngine::registry().map_err(common::unavailable)?;
        let entry = registry
            .installed_tools
            .get_mut(&target.name)
            .ok_or_else(|| common::missing(&target))?;
        if entry.category != target.kind.as_str() {
            return Err(common::missing(&target));
        }
        if let Some(value) = params.patch.enabled {
            entry.enabled = value;
        }
        if let Some(value) = security {
            entry.security_mode = value;
        }
        if let Some(value) = execution {
            entry.execution_mode = value;
        }
        if let Some(value) = params.patch.trust_annotations {
            entry.trust_annotations = value;
        }
        if let Some(values) = &params.patch.user_settings {
            let dir = ToolsEngine::resolve_component_dir(target.kind.as_str(), &target.name).ok_or_else(|| common::missing(&target))?;
            let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
            let declared = cluaiz_agents::plugins::net::user_settings(target.kind.as_str(), &dir, &config_dir);
            // All names are checked before any is saved: a bad patch changes nothing
            if let Some(unknown) = values.keys().find(|name| !declared.iter().any(|setting| &setting.name == *name)) {
                let names: Vec<&str> = declared.iter().map(|setting| setting.name.as_str()).collect();
                return Err(roots::invalid(
                    "patch.user_settings",
                    &format!("'{unknown}' is not a setting of this tool; its settings: {}", if names.is_empty() { "none".into() } else { names.join(", ") }),
                ));
            }
            for (setting, value) in values {
                cluaiz_agents::plugins::net::save_user_setting(target.kind.as_str(), &dir, setting, value, &config_dir)
                    .map_err(common::unavailable)?;
            }
        }
        if let Some(values) = &params.patch.permissions {
            let declared = cluaiz_agents::permissions::declared(&manifest(entry));
            // All kinds are checked before any is saved: a bad patch changes nothing
            if let Some(unknown) = values.keys().find(|kind| !declared.iter().any(|p| &p.kind == *kind)) {
                let kinds: Vec<&str> = declared.iter().map(|p| p.kind.as_str()).collect();
                return Err(roots::invalid(
                    "patch.permissions",
                    &format!("'{unknown}' is not a permission this tool asks for; it asks for: {}", if kinds.is_empty() { "none".into() } else { kinds.join(", ") }),
                ));
            }
            let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
            for (kind, on) in values {
                cluaiz_agents::permissions::set(&config_dir, target.kind.as_str(), &target.name, kind, *on).map_err(common::unavailable)?;
            }
        }
        let result = settings(entry, &target);
        registry.save().map_err(common::unavailable)?;
        Ok(result)
    })
    .await?;
    if stop {
        ToolsEngine::stop_mcp_server_by_name(&name).await;
    }
    Ok(value)
}
