//! `flows.enable` and `flows.disable`: a flow's triggers on or off (docs/automation/methods/).

use super::common::{self, owner_confirms, typed, ScopeName};
use crate::service;
use crate::triggers;
use cluaiz_api::cep::CallCtx;
use cluaiz_automation::validate;
use cluaiz_cep::error as code;
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnableParams {
    /// The flow's name
    pub name: String,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// The owner accepts the permissions the file asks for
    pub permissions_confirmed: Option<bool>,
    /// Only these trigger ids (default all)
    pub triggers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisableParams {
    /// The flow's name
    pub name: String,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// Also cancel runs in progress (default false)
    pub cancel_running: Option<bool>,
    /// Only these trigger ids (default all)
    pub triggers: Option<Vec<String>>,
}

/// Trigger ids switched off one by one, as kept in the flow's settings
fn switched_off(settings: &Value) -> Vec<String> {
    settings["disabled_triggers"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default()
}

fn known_ids(flow: &Value, asked: &[String]) -> Result<(), CepError> {
    let ids: Vec<String> = cluaiz_automation::trigger::specs(flow).into_iter().map(|s| s.id).collect();
    match asked.iter().find(|id| !ids.contains(id)) {
        Some(id) => Err(CepError::invalid_params(format!("the flow has no trigger `{id}`; its triggers are {}", ids.join(", ")), vec![json!({ "path": "triggers", "message": "unknown trigger id" })], None)),
        None => Ok(()),
    }
}

pub async fn enable(ctx: CallCtx, call: Call, params: EnableParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let file = common::find(&store, &params.name, common::scope_of(params.scope))?;
    let flow = file.flow.clone().map_err(|problem| common::invalid_flow(&[problem]))?;
    let report = validate::validate(&flow, Some(&file.name), &service.snapshot().await);
    if !report.valid {
        return Err(common::invalid_flow(&report.problems));
    }
    let mut settings = store.settings(file.scope, &file.name);
    if let Some(wanted) = service::unconfirmed(&flow, &report.permissions_needed, &settings["permissions"]) {
        if !params.permissions_confirmed.unwrap_or(false) {
            return Err(typed(code::CONFLICT, "permissions_unconfirmed", "the file asks for permissions the owner has not confirmed; show them and call again with permissions_confirmed: true", json!({ "permissions_needed": wanted, "permissions_missing": report.permissions_missing })));
        }
        owner_confirms(&call, &format!("let the flow {} use {wanted}", file.name))?;
        settings["permissions"] = wanted;
    }
    let mut off = switched_off(&settings);
    match &params.triggers {
        Some(ids) => {
            known_ids(&flow, ids)?;
            off.retain(|id| !ids.contains(id));
        }
        None => off.clear(),
    }
    settings["enabled"] = json!(true);
    settings["disabled_triggers"] = json!(off);
    settings["hash"] = json!(report.hash);
    store.set_settings(file.scope, &file.name, &settings).map_err(|e| common::io("cannot keep the flow's settings", e))?;
    triggers::sync(&service, file.scope, &file.name, false).await;
    let summary = common::summary(&service, &store, &file, None);
    Ok(json!({ "triggers": summary["triggers"], "flow": summary }))
}

pub async fn disable(ctx: CallCtx, _call: Call, params: DisableParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let file = common::find(&store, &params.name, common::scope_of(params.scope))?;
    let mut settings = store.settings(file.scope, &file.name);
    match &params.triggers {
        Some(ids) => {
            if let Ok(flow) = &file.flow {
                known_ids(flow, ids)?;
            }
            let mut off = switched_off(&settings);
            off.extend(ids.iter().filter(|id| !off.contains(id)).cloned().collect::<Vec<_>>());
            settings["disabled_triggers"] = json!(off);
        }
        None => settings["enabled"] = json!(false),
    }
    store.set_settings(file.scope, &file.name, &settings).map_err(|e| common::io("cannot keep the flow's settings", e))?;
    triggers::sync(&service, file.scope, &file.name, false).await;
    let cancelled = if params.cancel_running.unwrap_or(false) { service.cancel_flow(file.scope.as_str(), &file.name) } else { 0 };
    Ok(json!({ "flow": common::summary(&service, &store, &file, None), "runs_cancelled": cancelled }))
}
