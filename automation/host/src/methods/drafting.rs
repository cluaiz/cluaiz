//! `flows.draft` and `flows.fix`: a model writes or corrects a flow, Automation checks it and
//! gives the problems back, and nothing is saved (docs/automation/methods/flows.draft.md,
//! flows.fix.md). Both run as CEP tasks.

use super::common::{typed, FormatName};
use super::{MODEL_FAILED, NOT_FOUND};
use crate::engine::EngineDoor;
use crate::service::{self, Launch};
use cluaiz_api::cep::CallCtx;
use cluaiz_automation::ease::{draft, explain};
use cluaiz_automation::format::Format;
use cluaiz_automation::host::{AskAnswer, AskRequest, StepHost};
use cluaiz_automation::journal::{self, RunStatus};
use cluaiz_automation::problem::Problem;
use cluaiz_automation::store::Scope;
use cluaiz_automation::{permissions, validate, FlowError, Kind};
use cluaiz_cep::error as code;
use cluaiz_cep::{Call, CepError, Progress, TaskHandle};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// A draft's or fix's task is kept this long
const TASK_TTL_MS: u64 = 60 * 60 * 1000;
/// The longest a draft waits for its test run
const TEST_WAIT_MS: u64 = 120_000;
/// A step record's input or output is shortened to this many characters for the model
const RECORD_CHARS: usize = 4_000;
/// Correction rounds of a fix
const FIX_ROUNDS: usize = 2;
/// How long a rate-limited model is waited for, once
const RATE_LIMIT_WAIT: std::time::Duration = std::time::Duration::from_secs(35);

/// A model's error as the method's
pub fn model_error(error: FlowError) -> CepError {
    let status = error.details.get("status").and_then(Value::as_u64);
    let kind = error.details.get("type").and_then(Value::as_str);
    if status == Some(404) || kind == Some("model_not_found") || error.kind == "not_found" {
        return typed(NOT_FOUND, "model_not_found", error.message, json!({}));
    }
    typed(code::UNAVAILABLE, "model_unavailable", format!("the model did not answer: {}", error.message), json!({ "kind": error.kind }))
}

/// One conversation with a model. A provider that refuses a schema for the answer is asked
/// again without one (the answer is checked either way).
pub struct Talk<'a> {
    door: &'a EngineDoor,
    model: Option<String>,
    schema_refused: bool,
    pub used_model: String,
}

impl<'a> Talk<'a> {
    pub fn new(door: &'a EngineDoor, model: Option<String>) -> Self {
        Talk { door, model, schema_refused: false, used_model: String::new() }
    }

    pub async fn ask(&mut self, messages: &[Value], schema: Option<&Value>) -> Result<AskAnswer, FlowError> {
        let info = cluaiz_automation::host::RunInfo { flow: "draft".into(), scope: "user".into(), run_id: String::new(), permissions: Value::Null, flow_dir: None };
        let request = |schema: Option<&Value>| AskRequest { model: self.model.clone(), messages: messages.to_vec(), output_schema: schema.cloned(), temperature: Some(0.2), ..Default::default() };
        let schema = if self.schema_refused { None } else { schema };
        let answer = match self.once(&info, request(schema)).await {
            // A provider that refuses the schema, or whose check of the answer against it fails,
            // is asked again without one: Automation checks the answer itself
            Err(error) if schema.is_some() && !rate_limited(&error) && matches!(error.kind.as_str(), "invalid_input" | "provider" | "model") => {
                tracing::info!("Automation: the model's answer with a schema failed ({}); asking without one", error.message);
                self.schema_refused = true;
                self.once(&info, request(None)).await?
            }
            other => other?,
        };
        self.used_model = answer.model.clone();
        Ok(answer)
    }

    /// The rest of the conversation goes without a schema for the answer
    pub fn drop_schema(&mut self) {
        if !self.schema_refused {
            tracing::info!("Automation: the answer held to a schema came back empty; asking without one");
        }
        self.schema_refused = true;
    }

    /// One question; a provider's rate limit per minute is waited out once
    async fn once(&self, info: &cluaiz_automation::host::RunInfo, request: AskRequest) -> Result<AskAnswer, FlowError> {
        match self.door.ask(info, request.clone()).await {
            Err(error) if rate_limited(&error) => {
                tracing::info!("Automation: the model is rate limited; asking again in {} s", RATE_LIMIT_WAIT.as_secs());
                tokio::time::sleep(RATE_LIMIT_WAIT).await;
                self.door.ask(info, request).await
            }
            other => other,
        }
    }
}

/// A limit per minute, which a short wait gets past. A spending cap or a used-up quota is not one.
fn rate_limited(error: &FlowError) -> bool {
    let text = error.message.to_lowercase();
    let lasting = ["spending cap", "spend cap", "billing", "quota exceeded", "exceeded your current quota"].iter().any(|w| text.contains(w));
    (error.kind == "rate_limit" || text.contains("(429)")) && !lasting
}

fn user(text: impl Into<String>) -> Value {
    json!({ "role": "user", "content": text.into() })
}

fn assistant(text: &str) -> Value {
    json!({ "role": "assistant", "content": text })
}

/// The language an explanation is written in
pub enum Language<'a> {
    /// Named by the caller (`Hindi`, `de`)
    Named(&'a str),
    /// The language these words of the person are in
    Of(&'a str),
}

impl Language<'_> {
    fn instruction(&self) -> String {
        match self {
            Language::Named(name) => format!("in {name}"),
            Language::Of(words) => format!(
                "in the language the person wrote their request in. Their words were: \u{201c}{}\u{201d}. Write every text in that language and script; write English only when those words are English",
                words.trim()
            ),
        }
    }
}

/// A plain explanation rewritten by a model in `language`; the facts stay. Fields the model
/// leaves out or changes in number keep the plain text.
pub async fn rewrite(talk: &mut Talk<'_>, plain: Value, language: Language<'_>) -> Result<(Value, String), FlowError> {
    let language = language.instruction();
    let shape = json!({
        "type": "object",
        "required": ["summary", "triggers", "steps"],
        "properties": {
            "summary": { "type": "string" },
            "triggers": { "type": "array", "items": { "type": "string" } },
            "steps": { "type": "array", "items": { "type": "object", "required": ["id", "text"], "properties": { "id": { "type": "string" }, "text": { "type": "string" } } } },
            "touches": { "type": "array", "items": { "type": "string" } },
            "warnings": { "type": "array", "items": { "type": "string" } }
        }
    });
    let steps: Vec<Value> = plain["steps"].as_array().into_iter().flatten().map(|s| json!({ "id": s["id"], "text": s["text"] })).collect();
    let given = json!({ "summary": plain["summary"], "triggers": plain["triggers"], "steps": steps, "touches": plain["touches"], "warnings": plain["warnings"] });
    let messages = vec![
        json!({ "role": "system", "content": format!("You explain automation flows to people. Rewrite the explanation you are given naturally and briefly, {language}. Keep every fact, time and number; keep step ids, tool and method names (such as fs.read or mcp:github/list_issues), event names and paths exactly as written, never translated; add nothing. Answer with JSON of the same shape only.") }),
        user(given.to_string()),
    ];
    let answer = talk.ask(&messages, Some(&shape)).await?;
    let Some(written) = draft::read_answer(&answer.text) else {
        return Err(FlowError::new(Kind::InvalidOutput, "the model's explanation was not JSON"));
    };
    let mut out = plain;
    if let Some(summary) = written["summary"].as_str().filter(|s| !s.trim().is_empty()) {
        out["summary"] = json!(summary);
    }
    for field in ["triggers", "touches", "warnings"] {
        if written[field].as_array().map(Vec::len) == out[field].as_array().map(Vec::len) && written[field].is_array() {
            out[field] = written[field].clone();
        }
    }
    let texts: Map<String, Value> = written["steps"].as_array().into_iter().flatten().filter_map(|s| Some((s["id"].as_str()?.to_string(), s["text"].clone()))).collect();
    for step in out["steps"].as_array_mut().into_iter().flatten() {
        if let Some(text) = step["id"].as_str().and_then(|id| texts.get(id)).filter(|t| t.is_string()) {
            step["text"] = text.clone();
        }
    }
    Ok((out, talk.used_model.clone()))
}

fn errors(problems: &[Problem]) -> Vec<&Problem> {
    problems.iter().filter(|p| p.is_error()).collect()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftParams {
    /// What the flow should do, in plain words, in any language
    pub request: String,
    /// The model that writes the flow; default the default chat model
    pub model: Option<String>,
    /// Only these nodes may be used (default every installed node); patterns such as `fs.*`
    pub tools: Option<Vec<String>>,
    /// An existing flow to change instead of writing a new one; `request` says what to change
    pub flow: Option<Value>,
    /// The format of the returned `text` (default yaml)
    pub format: Option<FormatName>,
    /// Run it once as a dry run (default true)
    pub test: Option<bool>,
    /// Sample trigger data for the test
    pub sample: Option<Value>,
    /// Correction rounds after validation, 0–5 (default 3)
    pub rounds: Option<u32>,
    /// Language of the explanation (default: the request's)
    pub language: Option<String>,
}

pub async fn draft(ctx: CallCtx, call: Call, params: DraftParams) -> Result<Value, CepError> {
    if params.request.trim().is_empty() {
        return Err(CepError::invalid_params("`request` says what the flow should do", vec![json!({ "path": "request", "message": "empty" })], None));
    }
    let state = ctx.state.clone();
    let principal = call.principal.clone();
    let view = ctx.state.cep.shared().tasks.start(&call.principal.id, "flows.draft", Some(TASK_TTL_MS), move |handle| async move { write_draft(state, principal, params, handle).await })?;
    Ok(json!({ "task": view }))
}

async fn write_draft(state: Arc<cluaiz_api::AppState>, principal: Arc<cluaiz_cep::Principal>, params: DraftParams, handle: TaskHandle) -> Result<Value, CepError> {
    let service = service::get(&state);
    let catalog = service.snapshot().await;
    if let Some(model) = &params.model {
        if cluaiz_automation::host::Catalog::model_installed(&catalog, model) == Some(false) {
            return Err(typed(NOT_FOUND, "model_not_found", format!("the model {model} is not installed"), json!({})));
        }
    }
    let mut nodes = catalog.nodes();
    if let Some(only) = &params.tools {
        nodes.retain(|n| n["ref"].as_str().is_some_and(|r| only.iter().any(|p| permissions::pattern_matches(p, r))));
    }
    let door = EngineDoor { state: state.clone(), catalog };
    let mut talk = Talk::new(&door, params.model.clone());
    let ask_for = match &params.flow {
        Some(existing) => format!("Change this flow as asked: {}\n\nThe flow:\n{}", params.request.trim(), serde_json::to_string_pretty(existing).unwrap_or_default()),
        None => format!("Write a flow that does this: {}", params.request.trim()),
    };
    let mut messages = vec![json!({ "role": "system", "content": draft::guide(&nodes) }), user(ask_for)];
    let rounds = params.rounds.unwrap_or(3).min(5) as u64;
    let schema = draft::flow_answer_schema();
    let mut last_text = String::new();
    let mut best: Option<(Value, validate::Report)> = None;
    for round in 0..=rounds {
        handle.progress(Progress { done: round, total: Some(rounds + 1), unit: Some("rounds".into()), message: Some(if round == 0 { "writing the flow".into() } else { "correcting the problems".into() }) });
        if handle.cancel_flag().load(std::sync::atomic::Ordering::Relaxed) {
            return Err(CepError::cancelled());
        }
        let answer = talk.ask(&messages, Some(&schema)).await.map_err(model_error)?;
        last_text = answer.text.clone();
        messages.push(assistant(&answer.text));
        let Some(flow) = draft::read_answer(&answer.text).map(draft::flow_of) else {
            messages.push(user("That was not one JSON object. Answer with the whole flow as one JSON object only."));
            continue;
        };
        if draft::hollow(&flow) {
            // Some providers fill a schema's open objects with nothing: ask again without one
            talk.drop_schema();
            messages.pop();
            continue;
        }
        let report = validate::validate(&flow, None, &door.catalog);
        let valid = report.valid;
        if !valid {
            messages.push(user(draft::correction(&report.problems)));
        }
        best = Some((flow, report));
        if valid {
            break;
        }
    }
    let Some((flow, report)) = best else {
        return Err(typed(MODEL_FAILED, "draft_failed", "the model gave no flow as JSON; try a clearer request or a larger model", json!({ "problems": [], "text": last_text })));
    };
    if !report.valid && !draft::only_missing(&report.problems) {
        let problems: Vec<&Problem> = errors(&report.problems);
        return Err(typed(MODEL_FAILED, "draft_failed", format!("after {rounds} correction rounds the flow still has {} error(s); see data.problems", problems.len()), json!({ "problems": report.problems, "text": last_text, "flow": flow })));
    }
    // The test: a dry run, only for the owner (a draft's reads use permissions nobody confirmed yet)
    let mut test = Value::Null;
    let mut test_run_id = Value::Null;
    if params.test.unwrap_or(true) && report.valid {
        if principal.owner {
            handle.progress(Progress { done: rounds + 1, total: Some(rounds + 2), unit: Some("rounds".into()), message: Some("testing it as a dry run".into()) });
            let (run_id, outcome) = test_run(&service, &principal.id, &flow, &report, params.sample.clone()).await;
            test_run_id = json!(run_id);
            test = outcome;
        } else {
            test = json!({ "skipped": "only the owner's drafts are test-run; run it with flows.run and dry_run: true after saving" });
        }
    }
    let plain = explain::explain(&flow, &report, &door.catalog);
    let language = match params.language.as_deref() {
        Some(name) => Language::Named(name),
        None => Language::Of(&params.request),
    };
    let explanation = match rewrite(&mut talk, plain.clone(), language).await {
        Ok((written, _)) => written,
        Err(error) => {
            tracing::info!("Automation: the draft's explanation stays plain: {}", error.message);
            plain
        }
    };
    let format = params.format.map(|f| f.format()).unwrap_or(Format::Yaml);
    let text = draft::text_of(&flow, format);
    let permissions_needed = permissions::merge(flow.get("permissions").unwrap_or(&Value::Null), &report.permissions_needed);
    Ok(json!({
        "flow": flow,
        "text": text,
        "format": format.as_str(),
        "explanation": explanation,
        "problems": report.problems,
        "permissions_needed": permissions_needed,
        "missing": draft::missing(&report.problems),
        "test_run_id": test_run_id,
        "test": test,
        "model": talk.used_model,
    }))
}

/// Runs a draft once as a dry run and waits for it: `(run id, { status, error, steps_done })`
async fn test_run(service: &Arc<service::Service>, principal: &str, flow: &Value, report: &validate::Report, sample: Option<Value>) -> (Option<String>, Value) {
    let mut trigger = sample.unwrap_or_else(|| draft::sample_trigger(flow));
    if let Some(map) = trigger.as_object_mut() {
        map.entry("kind").or_insert(json!("manual"));
        map.insert("by".into(), json!("draft"));
    }
    let launch = Launch {
        flow: flow.clone(),
        scope: Scope::User,
        permissions: permissions::merge(flow.get("permissions").unwrap_or(&Value::Null), &report.permissions_needed),
        inputs: draft::sample_inputs(flow),
        trigger,
        dry_run: true,
        mocks: Map::new(),
        mode: "parallel".into(),
        principal: principal.to_string(),
        parent: Value::Null,
        flow_dir: None,
    };
    let catalog = service.snapshot().await;
    let launched = match service.launch(service.runner(catalog), launch) {
        Ok(launched) => launched,
        Err(error) => return (None, json!({ "status": "not_started", "error": error.message })),
    };
    let task_id = launched.task.get("task_id").and_then(Value::as_str).unwrap_or_default().to_string();
    let record = service.state.cep.shared().tasks.result(principal, &task_id, TEST_WAIT_MS).await;
    let outcome = match record {
        Ok(record) => json!({ "status": record["status"], "error": record["error"], "steps_done": record["steps_done"] }),
        Err(error) if error.code == code::TIMEOUT => json!({ "status": "running", "note": "the dry run takes longer than two minutes; see flows.run_get" }),
        Err(error) => json!({ "status": "unknown", "error": error.message }),
    };
    (Some(launched.run_id), outcome)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixParams {
    /// A failed run
    pub run_id: Option<String>,
    /// A flow with problems (instead of `run_id`)
    pub flow: Option<Value>,
    /// What the person knows about the failure
    pub note: Option<String>,
    /// Default: the default chat model
    pub model: Option<String>,
}

fn shortened(value: &Value) -> Value {
    let text = value.to_string();
    if text.chars().count() <= RECORD_CHARS {
        return value.clone();
    }
    json!(format!("{}… (shortened)", text.chars().take(RECORD_CHARS).collect::<String>()))
}

/// What the model is told: the flow and what went wrong
struct Failure {
    flow: Value,
    name: Option<String>,
    what: Value,
}

fn failure(service: &service::Service, params: &FixParams, catalog: &crate::catalog::Snapshot) -> Result<Result<Failure, Value>, CepError> {
    match (&params.run_id, &params.flow) {
        (Some(run_id), None) => {
            let dir = journal::find_run(&service.paths, run_id).ok_or_else(|| typed(NOT_FOUND, "run_not_found", format!("there is no run {run_id} (it may have been removed by retention)"), json!({})))?;
            let run = journal::read_run(&dir).ok_or_else(|| CepError::unavailable("automation_storage", "the run's record cannot be read"))?;
            if !matches!(run.status, RunStatus::Failed) {
                return Err(typed(code::CONFLICT, "run_not_failed", format!("run {run_id} did not fail ({})", serde_json::to_value(run.status).unwrap_or_default().as_str().unwrap_or("?")), json!({})));
            }
            let flow = journal::read_json(&dir.join("flow.json")).ok_or_else(|| CepError::unavailable("automation_storage", "the run's flow cannot be read"))?;
            let failed: Vec<Value> = journal::read_journal(&dir)
                .finished
                .iter()
                .filter(|r| serde_json::to_value(&r.status).ok() == Some(json!("failed")))
                .map(|r| {
                    let record = serde_json::to_value(r).unwrap_or_default();
                    json!({ "id": record["id"], "place": record["place"], "input": shortened(&record["input"]), "error": record["error"], "attempts": record["attempts"] })
                })
                .collect();
            let trigger = journal::read_json(&dir.join("trigger.json")).unwrap_or(Value::Null);
            Ok(Ok(Failure { name: Some(run.flow.clone()), flow, what: json!({ "run_error": run.error, "failed_steps": failed, "trigger": shortened(&trigger) }) }))
        }
        (None, Some(flow)) => {
            let report = validate::validate(flow, None, catalog);
            if report.valid {
                return Ok(Err(json!({ "diagnosis": "The flow has no problems the checks can find. To fix a failure, send the failed run's run_id.", "changes": [], "flow": flow, "problems": report.problems, "cause_outside_flow": false, "model": null })));
            }
            Ok(Ok(Failure { name: None, flow: flow.clone(), what: json!({ "problems": errors(&report.problems) }) }))
        }
        _ => Err(CepError::invalid_params("send `run_id` (a failed run) or `flow` (a flow with problems), not both", vec![], None)),
    }
}

pub async fn fix(ctx: CallCtx, call: Call, params: FixParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let catalog = service.snapshot().await;
    if let Some(model) = &params.model {
        if cluaiz_automation::host::Catalog::model_installed(&catalog, model) == Some(false) {
            return Err(typed(NOT_FOUND, "model_not_found", format!("the model {model} is not installed"), json!({})));
        }
    }
    // The run and the flow are read before the task starts, so a wrong run_id is answered at once
    let failure = match failure(&service, &params, &catalog)? {
        Ok(failure) => failure,
        Err(nothing_to_fix) => {
            let view = ctx.state.cep.shared().tasks.start(&call.principal.id, "flows.fix", Some(TASK_TTL_MS), move |_| async move { Ok(nothing_to_fix) })?;
            return Ok(json!({ "task": view }));
        }
    };
    let state = ctx.state.clone();
    let view = ctx.state.cep.shared().tasks.start(&call.principal.id, "flows.fix", Some(TASK_TTL_MS), move |handle| async move { write_fix(state, catalog, failure, params, handle).await })?;
    Ok(json!({ "task": view }))
}

async fn write_fix(state: Arc<cluaiz_api::AppState>, catalog: crate::catalog::Snapshot, failure: Failure, params: FixParams, handle: TaskHandle) -> Result<Value, CepError> {
    let nodes = catalog.nodes();
    let door = EngineDoor { state, catalog };
    let mut talk = Talk::new(&door, params.model.clone());
    // The model never sees or changes `permissions`: the owner confirms those, and they are put
    // back into the corrected flow as they were
    let permissions = failure.flow.get("permissions").cloned();
    let mut shown = failure.flow.clone();
    if let Some(map) = shown.as_object_mut() {
        map.remove("permissions");
    }
    let mut ask_for = format!(
        "This flow went wrong. Find why and correct it.\n\nThe flow:\n{}\n\nWhat happened:\n{}",
        serde_json::to_string_pretty(&shown).unwrap_or_default(),
        serde_json::to_string_pretty(&failure.what).unwrap_or_default()
    );
    if let Some(note) = params.note.as_deref().filter(|n| !n.trim().is_empty()) {
        ask_for.push_str(&format!("\n\nWhat the person knows: {}", note.trim()));
    }
    let mut messages = vec![json!({ "role": "system", "content": draft::fix_guide(&nodes) }), user(ask_for)];
    let schema = draft::fix_answer_schema();
    let mut last: Option<(Value, Value, validate::Report)> = None;
    let mut last_text = String::new();
    for round in 0..=FIX_ROUNDS {
        handle.progress(Progress { done: round as u64, total: Some(FIX_ROUNDS as u64 + 1), unit: Some("rounds".into()), message: Some(if round == 0 { "reading the failure".into() } else { "correcting the problems".into() }) });
        let answer = talk.ask(&messages, Some(&schema)).await.map_err(model_error)?;
        last_text = answer.text.clone();
        messages.push(assistant(&answer.text));
        let Some(reply) = draft::read_answer(&answer.text) else {
            messages.push(user("That was not one JSON object. Answer with { diagnosis, cause_outside_flow, reasons, flow } as one JSON object only."));
            continue;
        };
        if reply["cause_outside_flow"] == json!(true) {
            let report = validate::validate(&failure.flow, failure.name.as_deref(), &door.catalog);
            return Ok(json!({ "diagnosis": reply["diagnosis"], "changes": [], "flow": failure.flow, "problems": report.problems, "cause_outside_flow": true, "model": talk.used_model }));
        }
        let Some(mut fixed) = reply.get("flow").filter(|f| f.is_object()).cloned() else {
            messages.push(user("The answer has no `flow` object. Answer again with the whole corrected flow in `flow`."));
            continue;
        };
        if draft::hollow(&fixed) {
            // Some providers fill a schema's open objects with nothing: ask again without one
            talk.drop_schema();
            messages.pop();
            continue;
        }
        if let Some(map) = fixed.as_object_mut() {
            match &permissions {
                Some(permissions) => map.insert("permissions".into(), permissions.clone()),
                None => map.remove("permissions"),
            };
        }
        let report = validate::validate(&fixed, failure.name.as_deref(), &door.catalog);
        let valid = report.valid;
        if !valid {
            messages.push(user(draft::correction(&report.problems).replace("Write the whole flow again with them corrected, as one JSON object.", "Answer again with { diagnosis, cause_outside_flow, reasons, flow }, the flow corrected.")));
        }
        last = Some((reply, fixed, report));
        if valid {
            break;
        }
    }
    let Some((reply, fixed, report)) = last else {
        return Err(typed(MODEL_FAILED, "fix_failed", "the model gave no corrected flow as JSON; try a larger model, or edit the flow by hand", json!({ "text": last_text })));
    };
    let reasons: Vec<Value> = reply["reasons"].as_array().cloned().unwrap_or_default();
    let changes = draft::with_reasons(draft::changes(&failure.flow, &fixed), &reasons);
    Ok(json!({ "diagnosis": reply["diagnosis"], "changes": changes, "flow": fixed, "problems": report.problems, "cause_outside_flow": false, "model": talk.used_model }))
}
