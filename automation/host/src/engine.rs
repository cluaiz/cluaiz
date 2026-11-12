//! The StepHost: how a run's steps reach the Engine. Only public doors are used: CEP methods
//! through the dispatcher (scopes, folder roots, approvals and the audit log apply, as for any
//! caller), the chat pipeline for `ask`, and the approval store for questions to the owner.

use crate::catalog::Snapshot;
use async_trait::async_trait;
use axum::extract::State;
use axum::Json;
use cluaiz_api::cep::CallCtx;
use cluaiz_api::AppState;
use cluaiz_automation::host::{AskAnswer, AskRequest, Catalog, CodeCall, FlowCall, HostEvent, OwnerAnswer, OwnerQuestion, RunInfo, StepHost};
use cluaiz_automation::permissions::{self, SCOPE_ORDER};
use cluaiz_automation::validate::uses::parse_tool;
use cluaiz_automation::{FlowError, Kind};
use cluaiz_cep::envelope::Meta as CallMeta;
use cluaiz_cep::error as code;
use cluaiz_cep::{CepError, OnDenied, Principal, Scope};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long a step waits for a CEP task it started, in one `tasks.result` call
const TASK_POLL_MS: u64 = 30_000;
/// The longest answer body read from the chat pipeline
const MAX_ANSWER_BYTES: usize = 64 << 20;

pub struct EngineDoor {
    pub state: Arc<AppState>,
    pub catalog: Snapshot,
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(path),
    }
}

/// The run's principal: the flow's scopes (never admin) and its folders, plus the folders of the
/// tools it may run (the Engine checks a tool's folder against the caller's roots)
pub fn principal(run: &RunInfo, workspace: &Path) -> Principal {
    let list = |field: &str| -> Vec<String> { run.permissions.get(field).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect() };
    let scopes: Vec<Scope> = SCOPE_ORDER
        .iter()
        .filter(|s| permissions::allows_scope(&run.permissions, s))
        .map(|s| match *s {
            "write" => Scope::Write,
            "exec" => Scope::Exec,
            _ => Scope::Read,
        })
        .collect();
    let mut roots: Vec<PathBuf> = list("folders").iter().filter_map(|f| cluaiz_api::cep::roots::resolve(&expand_home(f).to_string_lossy(), workspace, false).ok()).collect();
    for tool in list("tools") {
        let dir = if tool == "*" {
            Some(cluaiz_core::EnvironmentManager::current().tools_dir())
        } else {
            let name = tool.trim_end_matches('*').trim_end_matches('/');
            parse_tool(&format!("{name}/x")).or_else(|_| parse_tool(name)).ok().and_then(|t| cluaiz_agents::ToolsEngine::resolve_component_dir(&t.kind, &t.name))
        };
        if let Some(root) = dir.and_then(|d| cluaiz_api::cep::roots::resolve(&d.to_string_lossy(), workspace, false).ok()) {
            roots.push(root);
        }
    }
    Principal { id: format!("flow:{}", run.flow), scopes, roots: Some(roots), on_denied: OnDenied::Error, owner: false }
}

/// A CEP error as a step error, with its `data` kept in `details`
pub fn step_error(error: CepError) -> FlowError {
    let kind = match error.code {
        code::INVALID_PARAMS => Kind::InvalidInput,
        code::NOT_FOUND | code::METHOD_NOT_FOUND => Kind::NotFound,
        code::FORBIDDEN | code::UNAUTHENTICATED => Kind::Permission,
        code::APPROVAL_REQUIRED => Kind::ApprovalRejected,
        code::CONFLICT => Kind::Conflict,
        code::TOO_LARGE => Kind::Limit,
        code::TIMEOUT => Kind::Timeout,
        code::CANCELLED => Kind::Cancelled,
        _ => Kind::Unavailable,
    };
    let data = error.data.clone().unwrap_or(Value::Null);
    let kind = if data.get("type").and_then(Value::as_str) == Some("tool_operation_failed") { Kind::Tool } else { kind };
    FlowError::new(kind, error.message).with_details(data)
}

impl EngineDoor {
    async fn workspace(&self) -> PathBuf {
        self.state.current_workspace_dir.read().await.clone()
    }

    async fn invoke(&self, principal: &Arc<Principal>, method: &str, params: Map<String, Value>, approval: Option<String>) -> Result<Value, CepError> {
        let ctx = CallCtx { state: self.state.clone() };
        self.state.cep.invoke(ctx, principal.clone(), method, params, CallMeta { approval, ..Default::default() }).await
    }

    /// Waits for the owner's answer to an approval, as long as the approval is open; `None` when
    /// nobody answered in time
    pub(crate) async fn wait_for(&self, approval_id: &str, expires_at: Option<&str>) -> Option<cluaiz_cep::approval::Decision> {
        let approvals = &self.state.cep.shared().approvals;
        let waiter = approvals.wait(approval_id);
        let left = expires_at.and_then(chrono_left_ms).unwrap_or(300_000);
        // Stops waiting (and forgets the question) also when the step is cancelled
        let forget = ForgetOnDrop { approvals, id: approval_id };
        let answer = match tokio::time::timeout(Duration::from_millis(left), waiter).await {
            Ok(Ok(decision)) => Some(decision),
            _ => None,
        };
        if answer.is_some() {
            std::mem::forget(forget);
        }
        answer
    }
}

/// Forgets a still-open approval when its waiter goes away (cancel, timeout)
struct ForgetOnDrop<'a> {
    approvals: &'a cluaiz_cep::ApprovalStore,
    id: &'a str,
}

impl Drop for ForgetOnDrop<'_> {
    fn drop(&mut self) {
        self.approvals.forget(self.id);
    }
}

/// Milliseconds until an RFC 3339 time
fn chrono_left_ms(expires_at: &str) -> Option<u64> {
    let at = cluaiz_automation::expr::values::parse_time(expires_at)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64;
    u64::try_from(at.timestamp_millis() - now).ok()
}

#[async_trait]
impl StepHost for EngineDoor {
    async fn call(&self, run: &RunInfo, method: &str, params: Value) -> Result<Value, FlowError> {
        let principal = Arc::new(principal(run, &self.workspace().await));
        let params = match params {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => return Err(FlowError::new(Kind::InvalidInput, format!("params must be an object, not {other}"))),
        };
        let mut outcome = self.invoke(&principal, method, params.clone(), None).await;
        // A call that needs the owner's approval waits for it, then goes on with that approval
        if let Err(error) = &outcome {
            if error.code == code::APPROVAL_REQUIRED {
                let data = error.data.clone().unwrap_or(Value::Null);
                let id = data.get("approval_id").and_then(Value::as_str).unwrap_or_default().to_string();
                if !self.wait_for(&id, data.get("expires_at").and_then(Value::as_str)).await.is_some_and(|d| d.approved) {
                    return Err(FlowError::new(Kind::ApprovalRejected, format!("the owner did not approve {method}")).with_details(data));
                }
                outcome = self.invoke(&principal, method, params, Some(id)).await;
            }
        }
        let result = outcome.map_err(step_error)?;
        // A method that runs as a task: wait for its result
        if let Some(task_id) = result.get("task").and_then(|t| t.get("task_id")).and_then(Value::as_str) {
            let tasks = &self.state.cep.shared().tasks;
            loop {
                match tasks.result(&principal.id, task_id, TASK_POLL_MS).await {
                    Err(error) if error.code == code::TIMEOUT => continue,
                    other => return other.map_err(step_error),
                }
            }
        }
        Ok(result)
    }

    async fn ask(&self, _run: &RunInfo, request: AskRequest) -> Result<AskAnswer, FlowError> {
        let messages = if request.messages.is_empty() { json!([{ "role": "system", "content": request.system }, { "role": "user", "content": request.user }]) } else { json!(request.messages) };
        let mut body = json!({
            "messages": messages,
            "stream": false,
            // A step keeps nothing in chat memory, and gets only the tools it names (an agent's,
            // which Automation runs itself: they come back as tool_calls)
            "temporary_chat": "lite",
            "tool_search": false,
            "tool_choice": if request.tools.is_empty() { "none" } else { "auto" },
        });
        if !request.tools.is_empty() {
            body["tools"] = json!(request.tools);
        }
        let fields = [
            ("model", request.model.clone().map(Value::from)),
            ("think_mode", request.think.clone().map(Value::from)),
            ("reasoning_effort", request.effort.clone().map(Value::from)),
            ("max_tokens", request.max_tokens.map(Value::from)),
            ("temperature", request.temperature.map(Value::from)),
            ("top_p", request.top_p.map(Value::from)),
            ("seed", request.seed.map(Value::from)),
            ("response_format", request.output_schema.clone().map(|schema| json!({ "type": "json_schema", "json_schema": { "name": "step_output", "schema": schema } }))),
        ];
        for (name, value) in fields {
            if let Some(value) = value {
                body[name] = value;
            }
        }
        let chat_request = serde_json::from_value(body).map_err(|e| FlowError::new(Kind::InvalidInput, format!("the ask request is not valid: {e}")))?;
        let response = cluaiz_api::handlers::chat::chat_completions(State(self.state.clone()), Json(chat_request)).await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), MAX_ANSWER_BYTES).await.map_err(|e| FlowError::new(Kind::Model, format!("the answer could not be read: {e}")))?;
        let answer: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        if !status.is_success() {
            let error = &answer["error"];
            let message = error["message"].as_str().unwrap_or("the model did not answer").to_string();
            let kind = match (status.as_u16(), error["type"].as_str()) {
                (429, _) => Kind::RateLimit,
                (_, Some("provider_error")) => Kind::Provider,
                (400, _) => Kind::InvalidInput,
                (413, _) => Kind::Limit,
                _ => Kind::Model,
            };
            return Err(FlowError::new(kind, message).with_details(json!({ "status": status.as_u16(), "type": error["type"] })));
        }
        let message = &answer["choices"][0]["message"];
        Ok(AskAnswer {
            text: message["content"].as_str().unwrap_or_default().to_string(),
            tool_calls: message["tool_calls"].as_array().cloned().unwrap_or_default(),
            thinking: message["reasoning_content"].as_str().map(String::from),
            model: answer["model"].as_str().unwrap_or_default().to_string(),
            prompt_tokens: answer["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: answer["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        })
    }

    async fn approval(&self, run: &RunInfo, question: OwnerQuestion) -> Result<OwnerAnswer, FlowError> {
        let approvals = &self.state.cep.shared().approvals;
        let digest = cluaiz_cep::approval::params_digest(&json!({ "run_id": run.run_id, "question": question.question, "target": question.target }));
        let shown = match &question.details {
            Value::Null => String::new(),
            details => format!(" — {}", crate::reach::short(details)),
        };
        let target = format!("{} ({}): {}{shown}", run.flow, run.run_id, question.question);
        let ttl = question.timeout_ms.map(Duration::from_millis);
        let approval = approvals.request_for(&format!("flow:{}", run.flow), "flows.run", digest, &target, ttl);
        let answer = self.wait_for(&approval.id, Some(&approval.expires_at)).await;
        Ok(OwnerAnswer { approved: answer.as_ref().map(|d| d.approved), note: answer.and_then(|d| d.feedback) })
    }

    fn catalog(&self) -> &dyn Catalog {
        &self.catalog
    }

    async fn emit(&self, run: &RunInfo, name: &str, data: Value, hops: u64) -> Result<u64, FlowError> {
        Ok(crate::reach::emit(&self.state, run, name, data, hops))
    }

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<HostEvent>> {
        Some(crate::reach::events())
    }

    async fn wait_webhook(&self, run: &RunInfo, token: &str, timeout_ms: u64) -> Result<Option<Value>, FlowError> {
        Ok(crate::reach::wait_webhook(&run.run_id, token, timeout_ms).await)
    }

    fn webhook_url(&self, run_id: &str, token: &str) -> Option<String> {
        Some(crate::reach::webhook_url(run_id, token))
    }

    async fn run_flow(&self, run: &RunInfo, call: FlowCall) -> Result<Value, FlowError> {
        crate::reach::run_flow(&self.state, run, call).await
    }

    async fn code(&self, run: &RunInfo, call: CodeCall) -> Result<Value, FlowError> {
        crate::reach::code(run, call).await
    }
}
