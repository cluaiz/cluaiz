//! What a run reaches beyond itself through the host: events (`emit`, and `wait` for one), the
//! run's own webhook address, other flows (`run`) and WebAssembly modules (`code`).

use crate::engine::step_error;
use crate::service::{self, Launch};
use axum::extract::{Path, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cluaiz_agents::plugins::net::NetAccess;
use cluaiz_agents::plugins::wasm::{self, Budget, ModuleError};
use cluaiz_api::AppState;
use cluaiz_automation::host::{CodeCall, FlowCall, HostEvent, RunInfo};
use cluaiz_automation::run::prepare_inputs;
use cluaiz_automation::store::Scope;
use cluaiz_automation::trigger::webhook;
use cluaiz_automation::{validate, FlowError, Kind};
use cluaiz_cep::error as code;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{broadcast, oneshot};

/// The Engine event that carries a flow's `emit`: `{ name, data, from }`
pub const EMITTED: &str = "flow.event";
/// A request to a run's own address, at most
const MAX_HOOK_BODY: usize = 1 << 20;
/// How long a step waits for another flow's run in one `tasks.result` call
const RESULT_WAIT_MS: u64 = 30_000;

/// A value shortened for a line of text (an approval's list entry)
pub fn short(value: &Value) -> String {
    let text = cluaiz_automation::refs::to_text(value);
    match text.char_indices().nth(300) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

/// Sends an event: flows with a matching `event` trigger start, and waiting runs see it. Answers
/// how many flows listen for it.
pub fn emit(state: &Arc<AppState>, run: &RunInfo, name: &str, data: Value, hops: u64) -> u64 {
    let from = json!({ "flow": run.flow, "scope": run.scope, "run_id": run.run_id });
    cluaiz_cep::events::publish(EMITTED, json!({ "name": name, "data": data, "from": from, "hops": hops }));
    let scope = Scope::parse(&run.scope).unwrap_or(Scope::User);
    service::get(state).triggers.listening_for_emit(name, scope, &run.flow).len() as u64
}

/// Engine events and flows' events, as runs see them
pub fn events() -> broadcast::Receiver<HostEvent> {
    static FORWARD: OnceLock<broadcast::Sender<HostEvent>> = OnceLock::new();
    FORWARD
        .get_or_init(|| {
            let (send, _) = broadcast::channel(1024);
            let mut engine = cluaiz_cep::events::subscribe();
            let forward = send.clone();
            tokio::spawn(async move {
                loop {
                    let event = match engine.recv().await {
                        Ok(event) => event,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return,
                    };
                    let seen = if event.name == EMITTED {
                        HostEvent { name: event.data["name"].as_str().unwrap_or_default().to_string(), data: event.data["data"].clone(), from: event.data["from"].clone() }
                    } else {
                        HostEvent { name: event.name, data: event.data, from: Value::Null }
                    };
                    let _ = forward.send(seen);
                }
            });
            send
        })
        .subscribe()
}

/// Runs waiting for a request to their own address: run id → (token, where the request goes)
fn waiting() -> std::sync::MutexGuard<'static, HashMap<String, (String, oneshot::Sender<Value>)>> {
    static WAITING: OnceLock<Mutex<HashMap<String, (String, oneshot::Sender<Value>)>>> = OnceLock::new();
    WAITING.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// Stops listening at the run's address when its wait ends in any way
struct Unlisten(String);

impl Drop for Unlisten {
    fn drop(&mut self) {
        waiting().remove(&self.0);
    }
}

pub async fn wait_webhook(run_id: &str, token: &str, timeout_ms: u64) -> Option<Value> {
    let (send, receive) = oneshot::channel();
    waiting().insert(run_id.to_string(), (token.to_string(), send));
    let _unlisten = Unlisten(run_id.to_string());
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), receive).await.ok().and_then(Result::ok)
}

/// The address an outside service calls: this server's address and the run's path
pub fn webhook_url(run_id: &str, token: &str) -> String {
    let settings = cluaiz_security::PermissionSchema::load();
    let host = std::env::var("CLUAIZ_HOST").unwrap_or_else(|_| settings.api_host.clone());
    let host = if host == "0.0.0.0" || host == "::" { "127.0.0.1".to_string() } else { host };
    format!("http://{host}:{}/v1/hooks/runs/{run_id}/{token}", cluaiz_api::api_port(&settings))
}

/// `POST /v1/hooks/runs/<run_id>/<token>`: once, while the run's `wait` step waits
pub async fn hook_route(Path((run_id, token)): Path<(String, String)>, request: Request) -> Response {
    let gone = || (StatusCode::NOT_FOUND, Json(json!({ "error": { "type": "no_hook", "message": "no run waits at this address" } }))).into_response();
    let proven = waiting().get(&run_id).is_some_and(|(expected, _)| webhook::same(expected.as_bytes(), token.as_bytes()));
    if !proven {
        return gone();
    }
    let method = request.method().to_string();
    let headers: Vec<(String, String)> = request.headers().iter().filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string()))).collect();
    let content_type = request.headers().get("content-type").and_then(|v| v.to_str().ok()).map(String::from);
    let query = request.uri().query().map(String::from);
    let Ok(raw) = axum::body::to_bytes(request.into_body(), MAX_HOOK_BODY).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({ "error": { "type": "too_large", "message": "the body is over 1 MB" } }))).into_response();
    };
    let body = match webhook::body(content_type.as_deref(), &raw) {
        Ok(body) => body,
        Err(error) => return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(json!({ "error": { "type": "unsupported_media_type", "message": format!("{error:?}") } }))).into_response(),
    };
    let kept: serde_json::Map<String, Value> = headers.into_iter().filter(|(k, _)| !matches!(k.as_str(), "authorization" | "cookie")).map(|(k, v)| (k.to_ascii_lowercase(), json!(v))).collect();
    let request = json!({ "method": method, "headers": kept, "query": webhook::form(query.as_deref().unwrap_or_default()), "body": body });
    let Some((_, send)) = waiting().remove(&run_id) else { return gone() };
    match send.send(request) {
        Ok(()) => (StatusCode::OK, Json(json!({ "accepted": true }))).into_response(),
        Err(_) => gone(),
    }
}

/// A `run` step: the other flow runs with its own permissions and record; its parent points back
pub async fn run_flow(state: &Arc<AppState>, run: &RunInfo, call: FlowCall) -> Result<Value, FlowError> {
    let service = service::get(state);
    let store = service.store().await;
    let scope = call.scope.as_deref().and_then(Scope::parse);
    let file = store.find(&call.name, scope).ok_or_else(|| FlowError::new(Kind::NotFound, format!("there is no saved flow named {}", call.name)))?;
    let flow = file.flow.clone().map_err(|problem| FlowError::new(Kind::InvalidInput, format!("the flow {} does not read: {}", call.name, problem.message)))?;
    let catalog = service.snapshot().await;
    let report = validate::validate(&flow, Some(&file.name), &catalog);
    if !report.valid {
        return Err(FlowError::new(Kind::InvalidInput, format!("the flow {} has errors; flows.validate lists them", call.name)));
    }
    let settings = store.settings(file.scope, &file.name);
    if service::unconfirmed(&flow, &report.permissions_needed, &settings["permissions"]).is_some() {
        return Err(FlowError::new(Kind::Permission, format!("the flow {} asks for permissions the owner has not confirmed", call.name)));
    }
    prepare_inputs(&flow, &call.inputs)?;
    let launch = Launch {
        flow,
        scope: file.scope,
        permissions: settings["permissions"].clone(),
        inputs: call.inputs.clone(),
        trigger: json!({ "kind": "run", "by": "run", "parent": call.parent }),
        dry_run: call.dry_run,
        mocks: Default::default(),
        // Another flow's run never waits behind its own caller
        mode: "parallel".into(),
        principal: "owner".into(),
        parent: call.parent.clone(),
        flow_dir: file.path.parent().map(PathBuf::from),
    };
    let launched = service.launch(service.runner(catalog), launch).map_err(step_error)?;
    tracing::info!("Automation: {} ({}) started {} as run {}", run.flow, run.run_id, call.name, launched.run_id);
    if !call.wait {
        return Ok(json!({ "run_id": launched.run_id }));
    }
    let task_id = launched.task.get("task_id").and_then(Value::as_str).unwrap_or_default().to_string();
    let tasks = &state.cep.shared().tasks;
    loop {
        match tasks.result("owner", &task_id, RESULT_WAIT_MS).await {
            Err(error) if error.code == code::TIMEOUT => continue,
            Err(error) => return Err(step_error(error)),
            Ok(record) if record["status"] == "succeeded" => return Ok(record["outputs"].clone()),
            Ok(record) => {
                let error = &record["error"];
                let kind = error["kind"].as_str().unwrap_or("failed").to_string();
                let message = format!("the flow {} {}: {}", call.name, record["status"].as_str().unwrap_or("failed"), error["message"].as_str().unwrap_or_default());
                return Err(FlowError { kind, message, step: None, details: json!({ "run_id": launched.run_id, "error": error }) });
            }
        }
    }
}

/// The module file: next to the flow file (no absolute paths, no `..`), or an installed plugin's
fn module_path(module: &str, flow_dir: Option<&std::path::Path>) -> Result<PathBuf, FlowError> {
    if let Some(name) = module.strip_prefix("plugin:") {
        let dir = cluaiz_agents::ToolsEngine::resolve_component_dir("plugin", name).ok_or_else(|| FlowError::new(Kind::NotFound, format!("the plugin {name} is not installed")))?;
        let found = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wasm")));
        return found.ok_or_else(|| FlowError::new(Kind::NotFound, format!("the plugin {name} has no .wasm module")));
    }
    let relative = std::path::Path::new(module);
    let escapes = relative.is_absolute() || relative.components().any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if escapes {
        return Err(FlowError::new(Kind::Permission, format!("`{module}`: a module is a path inside the flow file's folder, or plugin:<name>")));
    }
    let dir = flow_dir.ok_or_else(|| FlowError::new(Kind::InvalidInput, "an unsaved flow has no folder for its module; save it, or use plugin:<name>"))?;
    Ok(dir.join(relative))
}

/// A `code` step: the module runs inside the Engine with its limits, reaching only the flow's hosts
pub async fn code(run: &RunInfo, call: CodeCall) -> Result<Value, FlowError> {
    let path = module_path(&call.module, run.flow_dir.as_deref())?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| FlowError::new(Kind::NotFound, format!("cannot read the module {}: {e}", path.display())))?;
    let payload = serde_json::to_vec(&call.input).unwrap_or_default();
    let budget = Budget { function: call.function, memory_bytes: usize::try_from(call.memory_bytes).unwrap_or(usize::MAX), fuel: call.fuel };
    let network = NetAccess::for_hosts(call.network);
    let answer = tokio::task::spawn_blocking(move || wasm::run_module(&bytes, &payload, &budget, network)).await.map_err(|e| FlowError::new(Kind::Code, format!("the module stopped: {e}")))?;
    match answer {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| FlowError::new(Kind::InvalidOutput, format!("the module's answer is not JSON: {e}"))),
        Err(ModuleError::Limit(message)) => Err(FlowError::new(Kind::Limit, message)),
        Err(ModuleError::Failed(message)) => Err(FlowError::new(Kind::Code, message)),
    }
}
