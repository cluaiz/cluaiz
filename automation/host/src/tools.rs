//! Flows offered as tools: the `tool` trigger (docs/automation/triggers.md#tool). An enabled flow
//! with one is listed for MCP clients next to the Engine's methods as `flow.<name>` (MCP tool
//! names cannot hold the `:` of `flow:<name>`); its parameters are the flow's `inputs`, and a
//! call runs it and answers with its outputs. A flow that changes things asks the owner first.

use crate::catalog::Snapshot;
use crate::engine::EngineDoor;
use crate::service::{self, Service};
use crate::triggers::{self, Fired};
use cluaiz_api::cep::extensions::{Caller, CONFIRM_META};
use cluaiz_api::AppState;
use cluaiz_automation::host::Catalog;
use cluaiz_automation::journal::{self, RunStatus};
use cluaiz_automation::run::prepare_inputs;
use cluaiz_automation::store::{FlowFile, Scope};
use cluaiz_automation::trigger::{self, Spec};
use cluaiz_automation::validate::{self, Report};
use cluaiz_cep::error as code;
use cluaiz_cep::CepError;
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// MCP names of flows start with this
pub const PREFIX: &str = "flow.";
/// How often a call looks whether its run waits for a person
const LOOK_MS: u64 = 1_000;

/// A flow that is offered as a tool now
struct Offered {
    file: FlowFile,
    flow: Value,
    spec: Spec,
    report: Report,
}

/// Enabled flows with an enabled `tool` trigger, valid, with their permissions confirmed. A
/// project's flow hides the user's flow of the same name.
async fn offered(service: &Service, catalog: &Snapshot) -> Vec<Offered> {
    let store = service.store().await;
    let mut out: Vec<Offered> = Vec::new();
    for scope in [Scope::Project, Scope::User] {
        for file in store.list(Some(scope)) {
            if out.iter().any(|o| o.file.name == file.name) {
                continue;
            }
            let Ok(flow) = file.flow.clone() else { continue };
            let settings = store.settings(scope, &file.name);
            if settings["enabled"].as_bool() != Some(true) {
                continue;
            }
            let switched_off: Vec<&str> = settings["disabled_triggers"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let Some(spec) = trigger::specs(&flow).into_iter().find(|s| s.kind == "tool" && s.enabled() && !switched_off.contains(&s.id.as_str())) else { continue };
            let report = validate::validate(&flow, Some(&file.name), catalog);
            if !report.valid || service::unconfirmed(&flow, &report.permissions_needed, &settings["permissions"]).is_some() {
                continue;
            }
            out.push(Offered { file, flow, spec, report });
        }
    }
    out
}

/// The flow changes something: it runs tools or programs, calls a method that is not read-only,
/// or reaches the network
fn changes(offered: &Offered, catalog: &Snapshot) -> bool {
    let needed = &offered.report.needed;
    !needed.tools.is_empty()
        || needed.methods.iter().any(|m| !catalog.method(m).is_some_and(|i| i.read_only))
        || needed.scopes.iter().any(|s| s == "write" || s == "exec")
        || offered.flow.get("permissions").and_then(|p| p.get("network")).and_then(Value::as_array).is_some_and(|l| !l.is_empty())
}

/// The flow's inputs as a tool's parameters
fn parameters(flow: &Value) -> Value {
    let inputs = flow.get("inputs").and_then(Value::as_object).cloned().unwrap_or_default();
    let required: Vec<&String> = inputs.iter().filter(|(_, s)| s.get("default").is_none()).map(|(k, _)| k).collect();
    json!({ "type": "object", "properties": inputs, "required": required, "additionalProperties": false })
}

/// `tools/list` items for MCP clients
pub async fn list(state: Arc<AppState>) -> Vec<Value> {
    let service = service::get(&state);
    let catalog = service.snapshot().await;
    offered(&service, &catalog)
        .await
        .iter()
        .map(|o| {
            let name = &o.file.name;
            let description = o.spec.field("description").or_else(|| o.flow.get("description")).and_then(Value::as_str).map(String::from).unwrap_or_else(|| format!("Runs the flow {name}"));
            let title = o.spec.field("title").and_then(Value::as_str).unwrap_or(name);
            let changes = changes(o, &catalog);
            json!({
                "name": format!("{PREFIX}{name}"),
                "title": title,
                "description": description,
                "inputSchema": parameters(&o.flow),
                "annotations": { "title": title, "readOnlyHint": !changes, "destructiveHint": false, "idempotentHint": false, "openWorldHint": changes },
                "_meta": { CONFIRM_META: must_confirm(o.spec.field("confirm"), changes) },
            })
        })
        .collect()
}

/// Whether a call asks the owner first. `confirm` is `true` (always), `false` (never), or a mode:
/// `strict` always, `sandboxed` when the flow changes something, `full_access` never, and `auto`
/// the owner's agent security mode at the time of the call. Absent, it is `sandboxed`.
fn must_confirm(confirm: Option<&Value>, changes: bool) -> bool {
    let by_mode = |mode: &str| match mode {
        "strict" => true,
        "full_access" => false,
        _ => changes,
    };
    match confirm {
        Some(Value::Bool(ask)) => *ask,
        Some(Value::String(mode)) if mode == "auto" => by_mode(&cluaiz_security::PermissionSchema::load().agent_security_mode.to_lowercase()),
        Some(Value::String(mode)) => by_mode(mode),
        _ => changes,
    }
}

fn failed(kind: &str, message: impl Into<String>, data: Value) -> CepError {
    let mut data = if data.is_object() { data } else { json!({}) };
    data["type"] = json!(kind);
    CepError::new(code::UNAVAILABLE, message, Some(data))
}

/// Runs the flow behind `flow.<name>` for an MCP client and waits for its outputs. A run that
/// waits for a person answers with its id and status `waiting`.
pub async fn call(state: Arc<AppState>, tool: String, arguments: Map<String, Value>, caller: Caller) -> Result<Value, CepError> {
    let name = tool.strip_prefix(PREFIX).unwrap_or(&tool).to_string();
    let service = service::get(&state);
    let catalog = service.snapshot().await;
    let Some(offered) = offered(&service, &catalog).await.into_iter().find(|o| o.file.name == name) else {
        return Err(CepError::invalid_params(format!("Unknown tool: {tool} (no enabled flow {name} with a tool trigger)"), Vec::new(), None));
    };
    let inputs = Value::Object(arguments.clone());
    prepare_inputs(&offered.flow, &inputs).map_err(|error| {
        let errors = error.details.get("errors").and_then(Value::as_array).cloned().unwrap_or_default();
        CepError::invalid_params(error.message, errors, None)
    })?;
    let confirm = must_confirm(offered.spec.field("confirm"), changes(&offered, &catalog));
    // A chat asked the owner with its own approval card already
    if confirm && !caller.confirmed {
        let approvals = &state.cep.shared().approvals;
        let digest = cluaiz_cep::approval::params_digest(&json!({ "flow": name, "inputs": inputs }));
        let who = match caller.kind {
            "chat" => "a chat",
            "agent" => "the coding agent",
            _ => "an MCP client",
        };
        let target = format!("{who} runs the flow {name} with {}", crate::reach::short(&inputs));
        let approval = approvals.request_for(caller.kind, "flows.run", digest, &target, None);
        let door = EngineDoor { state: state.clone(), catalog };
        if !door.wait_for(&approval.id, Some(&approval.expires_at)).await.is_some_and(|d| d.approved) {
            return Err(CepError::new(code::APPROVAL_REQUIRED, format!("the owner did not approve running the flow {name}"), Some(json!({ "type": "approval_rejected", "approval_id": approval.id }))));
        }
    }
    let mut who = json!({ "kind": caller.kind });
    if let Some(chat_id) = &caller.chat_id {
        who["chat_id"] = json!(chat_id);
    }
    let data = json!({ "caller": who, "arguments": inputs });
    let launched = match triggers::fire(&service, offered.file.scope, &name, &offered.spec.id, data).await {
        Fired::Started(launched) => launched,
        Fired::Filtered => return Err(failed("filtered", format!("the tool trigger's filter did not let this call run {name}"), json!({}))),
        Fired::Duplicate => return Err(failed("duplicate", format!("the same call already ran {name} (dedupe)"), json!({}))),
        Fired::Skipped(message) => return Err(failed("skipped", message, json!({}))),
        Fired::Gone => return Err(failed("flow_gone", format!("the flow {name} was changed or turned off"), json!({}))),
        Fired::Failed(message) => return Err(failed("not_started", message, json!({}))),
    };
    let task_id = launched.task.get("task_id").and_then(Value::as_str).unwrap_or_default().to_string();
    let run_dir = service.paths.run_dir(&name, &launched.run_id);
    let tasks = &state.cep.shared().tasks;
    loop {
        match tasks.result("owner", &task_id, LOOK_MS).await {
            Ok(record) => return answer(record),
            Err(error) if error.code == code::TIMEOUT => {
                if let Some(run) = journal::read_run(&run_dir).filter(|r| matches!(r.status, RunStatus::Waiting)) {
                    return Ok(json!({ "run_id": run.run_id, "status": "waiting", "waiting_for": run.waiting_for, "note": "the run waits for a person; see flows.run_get" }));
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn answer(record: Value) -> Result<Value, CepError> {
    match record["status"].as_str() {
        Some("succeeded") => Ok(json!({ "run_id": record["run_id"], "status": "succeeded", "outputs": record["outputs"] })),
        status => {
            let message = record["error"]["message"].as_str().map(String::from).unwrap_or_else(|| format!("the run ended {}", status.unwrap_or("without a status")));
            Err(failed("flow_failed", message, json!({ "run_id": record["run_id"], "status": status, "error": record["error"] })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::must_confirm;
    use serde_json::json;

    #[test]
    fn confirm_follows_the_field_or_the_mode() {
        assert!(must_confirm(None, true) && !must_confirm(None, false));
        assert!(must_confirm(Some(&json!(true)), false) && !must_confirm(Some(&json!(false)), true));
        assert!(must_confirm(Some(&json!("strict")), false));
        assert!(!must_confirm(Some(&json!("full_access")), true));
        assert!(must_confirm(Some(&json!("sandboxed")), true) && !must_confirm(Some(&json!("sandboxed")), false));
    }
}
