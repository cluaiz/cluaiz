//! `flows.run`, `flows.runs`, `flows.run_get` (docs/automation/methods/).

use super::common::{self, owner_confirms, typed, ScopeName};
use crate::service::{self, concurrency, Launch};
use cluaiz_api::cep::CallCtx;
use cluaiz_automation::journal::{self, RunRecord};
use cluaiz_automation::permissions;
use cluaiz_automation::run::{prepare_inputs, RunRequest};
use cluaiz_automation::store::Scope;
use cluaiz_automation::validate;
use cluaiz_cep::error as code;
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunConcurrency {
    Parallel,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunParams {
    /// The saved flow to run
    pub name: Option<String>,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// An unsaved flow to run instead (testing from an editor)
    pub flow: Option<Value>,
    /// For an inline `flow`: the owner accepts its permissions
    pub permissions_confirmed: Option<bool>,
    /// The run's inputs, checked against the flow's `inputs`
    pub inputs: Option<Value>,
    /// Sample trigger data for a test; default `{ kind: "manual" }`
    pub trigger: Option<Value>,
    /// No side effects (default false)
    pub dry_run: Option<bool>,
    /// Step id → output to use instead of running that step
    pub mocks: Option<Map<String, Value>>,
    /// `parallel` runs even if another run of this flow is going
    pub concurrency: Option<RunConcurrency>,
    /// Report `step.output_delta` events for model steps (default false)
    pub stream: Option<bool>,
}

pub async fn run(ctx: CallCtx, call: Call, params: RunParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let store = service.store().await;
    let catalog = service.snapshot().await;
    let (flow, scope, expected, confirmed, flow_dir) = match (params.name, params.flow) {
        (Some(name), None) => {
            let file = common::find(&store, &name, common::scope_of(params.scope))?;
            let flow = file.flow.clone().map_err(|problem| common::invalid_flow(&[problem]))?;
            let confirmed = store.settings(file.scope, &file.name)["permissions"].clone();
            (flow, file.scope, Some(file.name), confirmed, file.path.parent().map(std::path::PathBuf::from))
        }
        (None, Some(flow)) => (flow, common::scope_of(params.scope).unwrap_or(Scope::User), None, Value::Null, None),
        _ => return Err(CepError::invalid_params("send `name` (a saved flow) or `flow` (an unsaved one), not both", vec![], None)),
    };
    let report = validate::validate(&flow, expected.as_deref(), &catalog);
    if !report.valid {
        return Err(common::invalid_flow(&report.problems));
    }
    let granted = if expected.is_some() {
        if let Some(wanted) = service::unconfirmed(&flow, &report.permissions_needed, &confirmed) {
            return Err(typed(code::CONFLICT, "permissions_unconfirmed", "the flow asks for permissions the owner has not confirmed (it was changed outside Automation, or never saved with flows.save); save it again with flows.save", json!({ "permissions_needed": wanted, "permissions_confirmed": confirmed })));
        }
        confirmed
    } else {
        let wanted = permissions::merge(flow.get("permissions").unwrap_or(&Value::Null), &report.permissions_needed);
        if !params.permissions_confirmed.unwrap_or(false) {
            return Err(typed(code::CONFLICT, "permissions_unconfirmed", "an unsaved flow runs only with permissions_confirmed: true", json!({ "permissions_needed": wanted })));
        }
        owner_confirms(&call, &format!("let an unsaved flow use {wanted}"))?;
        wanted
    };
    let inputs = params.inputs.unwrap_or(Value::Null);
    prepare_inputs(&flow, &inputs).map_err(|error| typed(code::INVALID_PARAMS, "invalid_input", error.message, error.details))?;
    if params.stream.unwrap_or(false) {
        tracing::info!("flows.run: stream is accepted; step.output_delta events are not built yet");
    }
    let mode = if params.concurrency == Some(RunConcurrency::Parallel) { "parallel".to_string() } else { concurrency(&flow) };
    let mut trigger = params.trigger.unwrap_or_else(|| json!({ "kind": "manual" }));
    if let Some(map) = trigger.as_object_mut() {
        map.entry("kind").or_insert(json!("manual"));
        map.entry("by").or_insert(json!(if call.principal.owner { "cep" } else { call.principal.id.as_str() }));
    }
    let launch = Launch {
        flow,
        scope,
        permissions: granted,
        inputs,
        trigger,
        dry_run: params.dry_run.unwrap_or(false),
        mocks: params.mocks.unwrap_or_default(),
        mode,
        principal: call.principal.id.clone(),
        parent: Value::Null,
        flow_dir,
    };
    let launched = service.launch(service.runner(catalog), launch)?;
    Ok(json!({ "task": launched.task, "run_id": launched.run_id, "status": launched.status }))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunsParams {
    /// One flow (default every flow)
    pub name: Option<String>,
    /// Which flow of that name. Absent: both
    pub scope: Option<ScopeName>,
    /// Only these statuses
    pub status: Option<Vec<String>>,
    /// Only runs created at or after this time
    pub since: Option<String>,
    /// Runs per page, 1–200 (default 20)
    pub limit: Option<u32>,
    /// The `next` cursor of the previous page
    pub after: Option<String>,
}

pub async fn runs(ctx: CallCtx, _call: Call, params: RunsParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    if let Some(name) = &params.name {
        let store = service.store().await;
        if store.find(name, common::scope_of(params.scope)).is_none() && journal::list_runs(&service.paths, Some(name)).is_empty() {
            return Err(common::not_found(name));
        }
    }
    let scope = params.scope.map(|s| s.scope().as_str());
    let since = params.since.as_deref().and_then(cluaiz_automation::expr::values::parse_time);
    let mut list: Vec<RunRecord> = journal::list_runs(&service.paths, params.name.as_deref())
        .into_iter()
        .filter(|r| scope.is_none_or(|s| r.scope == s))
        .filter(|r| params.status.as_ref().is_none_or(|wanted| wanted.iter().any(|w| serde_json::to_value(r.status).ok() == Some(json!(w)))))
        .filter(|r| since.is_none_or(|s| cluaiz_automation::expr::values::parse_time(&r.created_at).is_some_and(|c| c >= s)))
        .collect();
    if let Some(after) = &params.after {
        if let Some(at) = list.iter().position(|r| &r.run_id == after) {
            list.drain(..=at);
        }
    }
    let limit = params.limit.unwrap_or(20).clamp(1, 200) as usize;
    let next = (list.len() > limit).then(|| list[limit - 1].run_id.clone());
    list.truncate(limit);
    Ok(json!({ "runs": list, "next": next }))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunGetParams {
    /// The run
    pub run_id: String,
    /// Include step records (default true)
    pub steps: Option<bool>,
    /// Include inputs and outputs in step records (default true)
    pub io: Option<bool>,
    /// Only this step's records (all its items)
    pub step: Option<String>,
    /// Step records per page, 1–1000 (default 200)
    pub limit: Option<u32>,
    /// Cursor from the previous page
    pub after: Option<String>,
}

pub async fn run_get(ctx: CallCtx, _call: Call, params: RunGetParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let dir = journal::find_run(&service.paths, &params.run_id).ok_or_else(|| typed(code::NOT_FOUND, "run_not_found", format!("there is no run {} (it may have been removed by retention)", params.run_id), json!({})))?;
    let run = journal::read_run(&dir).ok_or_else(|| CepError::unavailable("automation_storage", "the run's record cannot be read"))?;
    let trigger = journal::read_json(&dir.join("trigger.json")).unwrap_or(Value::Null);
    let mut out = json!({ "run": run, "trigger": trigger });
    if params.steps.unwrap_or(true) {
        let journal = journal::read_journal(&dir);
        let mut records: Vec<Value> = journal.finished.iter().filter_map(|r| serde_json::to_value(r).ok()).collect();
        for (place, item) in &journal.unfinished {
            records.push(json!({ "place": place, "item": item, "status": if run.status.is_finished() { "unknown" } else { "running" } }));
        }
        if let Some(step) = &params.step {
            records.retain(|r| r["id"] == json!(step));
            if records.is_empty() {
                let known = journal::read_json(&dir.join("flow.json")).is_some_and(|f| f.to_string().contains(&format!("\"id\":\"{step}\"")));
                if !known {
                    return Err(typed(code::NOT_FOUND, "step_not_found", format!("`{step}` is not a step of this run's flow"), json!({})));
                }
            }
        }
        if !params.io.unwrap_or(true) {
            for record in &mut records {
                if let Some(map) = record.as_object_mut() {
                    map.remove("input");
                    map.remove("output");
                }
            }
        }
        let start = params.after.as_deref().and_then(|a| a.parse::<usize>().ok()).unwrap_or(0).min(records.len());
        let limit = params.limit.unwrap_or(200).clamp(1, 1000) as usize;
        let page: Vec<Value> = records.iter().skip(start).take(limit).cloned().collect();
        let next = (start + limit < records.len()).then(|| (start + limit).to_string());
        out["steps"] = json!(page);
        out["next"] = json!(next);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FlowVersion {
    /// The flow as the earlier run had it
    Original,
    /// The flow file as it is now
    Current,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RerunParams {
    /// The earlier run (any status but queued, running or waiting)
    pub run_id: String,
    /// The step id to start from
    pub from: String,
    /// Which flow to run (default current)
    pub flow_version: Option<FlowVersion>,
    /// Replace the run's inputs
    pub inputs: Option<Value>,
}

pub async fn rerun_from(ctx: CallCtx, call: Call, params: RerunParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let dir = journal::find_run(&service.paths, &params.run_id).ok_or_else(|| typed(code::NOT_FOUND, "run_not_found", format!("there is no run {}", params.run_id), json!({})))?;
    let earlier = journal::read_run(&dir).ok_or_else(|| CepError::unavailable("automation_storage", "the run's record cannot be read"))?;
    if !earlier.status.is_finished() {
        return Err(typed(code::CONFLICT, "run_not_finished", format!("run {} is still {:?}; cancel it or wait for it", earlier.run_id, earlier.status).to_lowercase(), json!({})));
    }
    let scope = Scope::parse(&earlier.scope).unwrap_or(Scope::User);
    let store = service.store().await;
    let file = store.find(&earlier.flow, Some(scope));
    let flow = match params.flow_version.unwrap_or(FlowVersion::Current) {
        FlowVersion::Original => journal::read_json(&dir.join("flow.json")).ok_or_else(|| CepError::unavailable("automation_storage", "the run's flow cannot be read"))?,
        FlowVersion::Current => file.as_ref().ok_or_else(|| common::not_found(&earlier.flow))?.flow.clone().map_err(|problem| common::invalid_flow(&[problem]))?,
    };
    let catalog = service.snapshot().await;
    let report = validate::validate(&flow, Some(&earlier.flow), &catalog);
    if !report.valid {
        return Err(common::invalid_flow(&report.problems));
    }
    let confirmed = store.settings(scope, &earlier.flow)["permissions"].clone();
    if let Some(wanted) = service::unconfirmed(&flow, &report.permissions_needed, &confirmed) {
        return Err(typed(code::CONFLICT, "permissions_unconfirmed", "the flow asks for permissions the owner has not confirmed; save it again with flows.save", json!({ "permissions_needed": wanted })));
    }
    let inputs = params.inputs.unwrap_or_else(|| earlier.inputs.clone());
    prepare_inputs(&flow, &inputs).map_err(|error| typed(code::INVALID_PARAMS, "invalid_input", error.message, error.details))?;
    let mut trigger = journal::read_json(&dir.join("trigger.json")).unwrap_or_else(|| json!({ "kind": "manual" }));
    trigger["by"] = json!("rerun");
    let mode = concurrency(&flow);
    let request = RunRequest {
        flow,
        scope: scope.as_str().to_string(),
        permissions: confirmed,
        inputs,
        trigger,
        dry_run: false,
        mocks: Map::new(),
        run_id: journal::new_run_id(),
        task_id: None,
        parent: earlier.parent.clone(),
        rerun_of: json!(earlier.run_id),
        flow_dir: file.as_ref().and_then(|f| f.path.parent().map(std::path::PathBuf::from)),
    };
    let runner = service.runner(catalog);
    let created = runner.rerun(request, &dir, &params.from).map_err(|error| match error.kind.as_str() {
        "not_found" => typed(code::NOT_FOUND, "step_not_found", error.message, json!({})),
        _ => CepError::unavailable("automation_storage", error.message),
    })?;
    let run_id = created.run_id().to_string();
    let task = service.start(runner, created, &call.principal.id, &mode)?;
    Ok(json!({ "task": task, "run_id": run_id, "rerun_of": earlier.run_id, "from": params.from }))
}
