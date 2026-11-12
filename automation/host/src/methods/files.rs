//! `flows.schema`, `flows.validate`, `flows.save`, `flows.get`, `flows.list`, `flows.remove`
//! (docs/automation/methods/).

use super::common::{self, given, owner_confirms, typed, FormatName, Given, ScopeName};
use crate::service;
use cluaiz_api::cep::CallCtx;
use cluaiz_automation::format::{json_schema, tables};
use cluaiz_automation::permissions;
use cluaiz_automation::store::{self, Scope};
use cluaiz_automation::validate::{self, Report};
use cluaiz_cep::error as code;
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchemaParams {
    /// A format version such as `cluaiz.flow/1`; default the newest
    pub version: Option<String>,
}

pub async fn schema(_ctx: CallCtx, _call: Call, params: SchemaParams) -> Result<Value, CepError> {
    let version = params.version.unwrap_or_else(|| tables::VERSION.to_string());
    if !tables::VERSIONS.contains(&version.as_str()) {
        return Err(typed(code::INVALID_PARAMS, "unknown_version", format!("`{version}` is not a format this release reads"), json!({ "versions": tables::VERSIONS })));
    }
    Ok(json!({ "version": version, "versions": tables::VERSIONS, "formats": ["json", "yaml"], "schema": json_schema::flow_schema() }))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateParams {
    /// The flow as JSON. Send `flow`, or `text` with `format`
    pub flow: Option<Value>,
    /// The flow as JSON or YAML text
    pub text: Option<String>,
    /// The format of `text`
    pub format: Option<FormatName>,
    /// Validate a saved flow instead (with `scope`)
    pub name: Option<String>,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
}

fn report_value(report: &Report) -> Value {
    serde_json::to_value(report).unwrap_or_default()
}

pub async fn validate(ctx: CallCtx, _call: Call, params: ValidateParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let (flow, expected) = match params.name {
        Some(name) => {
            let file = common::find(&store, &name, common::scope_of(params.scope))?;
            match file.flow {
                Ok(flow) => (flow, Some(file.name)),
                Err(problem) => return Ok(json!({ "valid": false, "problems": [problem] })),
            }
        }
        None => match given(params.flow, params.text, params.format)? {
            Given::Parsed { flow, .. } => (flow, None),
            Given::Broken(problem) => return Ok(json!({ "valid": false, "problems": [problem] })),
        },
    };
    let catalog = service.snapshot().await;
    Ok(report_value(&validate::validate(&flow, expected.as_deref(), &catalog)))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveParams {
    /// The flow as JSON. Send `flow`, or `text` with `format`
    pub flow: Option<Value>,
    /// The flow as JSON or YAML text (YAML comments are kept)
    pub text: Option<String>,
    /// The format of `text`
    pub format: Option<FormatName>,
    /// Where to save; default `user`. `project` needs a current workspace
    pub scope: Option<ScopeName>,
    /// Replace a flow of the same name; default false
    pub overwrite: Option<bool>,
    /// The owner accepted `permissions_needed` from the last answer
    pub permissions_confirmed: Option<bool>,
    /// Turn its triggers on after saving (default true)
    pub enable: Option<bool>,
}

pub async fn save(ctx: CallCtx, call: Call, params: SaveParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let (flow, text, format) = match given(params.flow, params.text, params.format)? {
        Given::Parsed { flow, text, format } => (flow, text, format),
        Given::Broken(problem) => return Err(common::invalid_flow(&[problem])),
    };
    let scope = common::scope_of(params.scope).unwrap_or(Scope::User);
    if store.dir(scope).is_none() {
        return Err(typed(code::CONFLICT, "no_workspace", "scope: project needs a current workspace (workspace.set)", json!({})));
    }
    let catalog = service.snapshot().await;
    let report = validate::validate(&flow, None, &catalog);
    if !report.valid {
        return Err(common::invalid_flow(&report.problems));
    }
    let name = flow["name"].as_str().unwrap_or_default().to_string();
    if let Some(existing) = store.find(&name, Some(scope)) {
        if !params.overwrite.unwrap_or(false) {
            return Err(typed(code::CONFLICT, "flow_exists", format!("a {} flow named {name} exists; send overwrite: true to replace it", scope.as_str()), json!({ "path": common::display(&existing.path) })));
        }
        owner_confirms(&call, &format!("replace the flow file {}", common::display(&existing.path)))?;
    }
    // What the owner must have confirmed: what the file lists, and what its steps use
    let wanted = permissions::merge(flow.get("permissions").unwrap_or(&Value::Null), &report.permissions_needed);
    let confirmed = store.settings(scope, &name)["permissions"].clone();
    if !permissions::covers(&confirmed, &wanted) {
        if !params.permissions_confirmed.unwrap_or(false) {
            return Err(typed(code::CONFLICT, "permissions_unconfirmed", "the flow needs permissions the owner has not confirmed; show permissions_needed to the owner and save again with permissions_confirmed: true", json!({ "permissions_needed": wanted, "permissions_missing": report.permissions_missing })));
        }
        owner_confirms(&call, &format!("let the flow {name} use {wanted}"))?;
    }
    let covered = flow.get("permissions").is_some_and(|p| permissions::covers(p, &wanted));
    let text = if covered { text } else { store::with_permissions(&text, format, &flow, &wanted) };
    let path = store.save(scope, &name, format, &text).map_err(|e| common::io("cannot write the flow file", e))?;
    let enabled = params.enable.unwrap_or(true);
    let switched_off = store.settings(scope, &name)["disabled_triggers"].clone();
    store.set_settings(scope, &name, &json!({ "enabled": enabled, "permissions": wanted, "hash": report.hash, "disabled_triggers": switched_off })).map_err(|e| common::io("cannot keep the flow's settings", e))?;
    crate::triggers::sync(&service, scope, &name, false).await;
    let file = store.find(&name, Some(scope)).ok_or_else(|| common::not_found(&name))?;
    // `permission_needed` is answered by this save: the file now lists what was confirmed
    let warnings: Vec<_> = report.problems.iter().filter(|p| !p.is_error() && p.code != "permission_needed").cloned().collect();
    tracing::info!("Automation: saved flow {name} ({}) at {}", scope.as_str(), path.display());
    Ok(json!({ "flow": common::summary(&service, &store, &file, Some(warnings.len())), "permissions": wanted, "problems": warnings }))
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Both,
    Text,
    Definition,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetParams {
    /// The flow's name
    pub name: String,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// What to return besides the summary (default both)
    #[serde(rename = "as")]
    pub view: Option<View>,
}

pub async fn get(ctx: CallCtx, _call: Call, params: GetParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let file = common::find(&store, &params.name, common::scope_of(params.scope))?;
    let warnings = match &file.flow {
        Ok(flow) => Some(validate::validate(flow, Some(&file.name), &service.snapshot().await).problems.iter().filter(|p| !p.is_error()).count()),
        Err(_) => None,
    };
    let view = params.view.unwrap_or(View::Both);
    let mut out = json!({ "flow": common::summary(&service, &store, &file, warnings), "format": file.format.as_str() });
    if view != View::Definition {
        out["text"] = json!(file.text);
    }
    if view != View::Text {
        out["definition"] = file.flow.clone().unwrap_or(Value::Null);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ListScope {
    Project,
    User,
    All,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    /// Which folders (default all)
    pub scope: Option<ListScope>,
    /// Only enabled, or only disabled flows
    pub enabled: Option<bool>,
    /// Only flows with this `meta.tags` entry
    pub tag: Option<String>,
    /// Words matched against names and descriptions
    pub query: Option<String>,
}

pub async fn list(ctx: CallCtx, _call: Call, params: ListParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let scope = match params.scope {
        Some(ListScope::Project) => Some(Scope::Project),
        Some(ListScope::User) => Some(Scope::User),
        _ => None,
    };
    let words: Vec<String> = params.query.as_deref().unwrap_or_default().split_whitespace().map(str::to_lowercase).collect();
    let flows: Vec<Value> = store
        .list(scope)
        .iter()
        .map(|file| common::summary(&service, &store, file, None))
        .filter(|s| params.enabled.is_none_or(|e| s["enabled"] == json!(e)))
        .filter(|s| params.tag.as_ref().is_none_or(|t| s["tags"].as_array().is_some_and(|tags| tags.iter().any(|x| x == t))))
        .filter(|s| {
            let text = format!("{} {}", s["name"].as_str().unwrap_or_default(), s["description"].as_str().unwrap_or_default()).to_lowercase();
            words.iter().all(|w| text.contains(w))
        })
        .collect();
    Ok(json!({ "flows": flows }))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    /// The flow's name
    pub name: String,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// Keep the run records (default false)
    pub keep_runs: Option<bool>,
    /// Cancel runs in progress first (default false)
    pub cancel_runs: Option<bool>,
}

pub async fn remove(ctx: CallCtx, call: Call, params: RemoveParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let file = common::find(&store, &params.name, common::scope_of(params.scope))?;
    let scope = file.scope.as_str();
    let running = service.running(scope, &file.name);
    let cancelled = if running.is_empty() {
        0
    } else if params.cancel_runs.unwrap_or(false) {
        service.cancel_flow(scope, &file.name)
    } else {
        return Err(typed(code::CONFLICT, "flow_running", format!("{} run(s) of {} are going; send cancel_runs: true to stop them", running.len(), file.name), json!({ "runs": running })));
    };
    owner_confirms(&call, &format!("delete the flow file {}", common::display(&file.path)))?;
    store.remove(&file).map_err(|e| common::io("cannot remove the flow file", e))?;
    service.triggers.stop(file.scope, &file.name);
    store.forget(file.scope, &file.name);
    let mut removed = 0;
    if !params.keep_runs.unwrap_or(false) {
        for run in cluaiz_automation::journal::list_runs(&service.paths, Some(&file.name)).into_iter().filter(|r| r.scope == scope && r.status.is_finished()) {
            if std::fs::remove_dir_all(service.paths.run_dir(&file.name, &run.run_id)).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(json!({ "removed": file.name, "scope": scope, "runs_removed": removed, "runs_cancelled": cancelled }))
}
