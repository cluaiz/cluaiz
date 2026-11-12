//! What the `flows.*` methods share: names and scopes, reading a flow from params, flow
//! summaries, typed errors, and the owner's confirmation of permissions.

use crate::service::Service;
use cluaiz_automation::format::{parse_text, Format};
use cluaiz_automation::journal;
use cluaiz_automation::problem::Problem;
use cluaiz_automation::store::{FlowFile, Scope, Store};
use cluaiz_cep::error as code;
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScopeName {
    Project,
    User,
}

impl ScopeName {
    pub fn scope(self) -> Scope {
        match self {
            ScopeName::Project => Scope::Project,
            ScopeName::User => Scope::User,
        }
    }
}

pub fn scope_of(name: Option<ScopeName>) -> Option<Scope> {
    name.map(ScopeName::scope)
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FormatName {
    Json,
    Yaml,
}

impl FormatName {
    pub fn format(self) -> Format {
        match self {
            FormatName::Json => Format::Json,
            FormatName::Yaml => Format::Yaml,
        }
    }
}

pub fn typed(code: i32, kind: &str, message: impl Into<String>, data: Value) -> CepError {
    let mut data = if data.is_object() { data } else { json!({}) };
    data["type"] = json!(kind);
    CepError::new(code, message, Some(data))
}

pub fn not_found(name: &str) -> CepError {
    typed(code::NOT_FOUND, "flow_not_found", format!("there is no flow named {name}"), json!({}))
}

pub fn invalid_flow(problems: &[Problem]) -> CepError {
    let errors = problems.iter().filter(|p| p.is_error()).count();
    typed(code::INVALID_PARAMS, "invalid_flow", format!("the flow has {errors} error(s); see data.problems"), json!({ "problems": problems }))
}

pub fn io(what: &str, error: std::io::Error) -> CepError {
    CepError::unavailable("automation_storage", format!("{what}: {error}"))
}

/// A flow given as `flow` (JSON) or as `text` with `format`; `Err` is a parse problem
pub enum Given {
    Parsed { flow: Value, text: String, format: Format },
    Broken(Problem),
}

pub fn given(flow: Option<Value>, text: Option<String>, format: Option<FormatName>) -> Result<Given, CepError> {
    match (flow, text) {
        (Some(flow), None) => {
            let text = serde_json::to_string_pretty(&flow).unwrap_or_default() + "\n";
            if text.len() > cluaiz_automation::format::parse::MAX_TEXT_BYTES {
                return Err(typed(code::TOO_LARGE, "too_large", "the flow is over 1 MB", json!({})));
            }
            Ok(Given::Parsed { flow, text, format: Format::Json })
        }
        (None, Some(text)) => {
            let format = format.ok_or_else(|| CepError::invalid_params("`format` (json or yaml) is required with `text`", vec![json!({ "path": "format", "message": "required with text" })], None))?.format();
            match parse_text(&text, format) {
                Ok(flow) => Ok(Given::Parsed { flow, text, format }),
                Err(problem) if problem.code == "too_large" => Err(typed(code::TOO_LARGE, "too_large", problem.message, json!({}))),
                Err(problem) => Ok(Given::Broken(problem)),
            }
        }
        _ => Err(CepError::invalid_params("send `flow` (JSON) or `text` with `format`, not both", vec![], None)),
    }
}

pub fn find(store: &Store, name: &str, scope: Option<Scope>) -> Result<FlowFile, CepError> {
    store.find(name, scope).ok_or_else(|| not_found(name))
}

/// A path for people: the home folder as `~`
pub fn display(path: &std::path::Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    match dirs::home_dir().map(|h| h.display().to_string().replace('\\', "/")) {
        Some(home) if text.starts_with(&home) => format!("~{}", &text[home.len()..]),
        _ => text,
    }
}

pub fn time_text(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The owner confirms: the owner directly, any other caller through an approval the owner gives
pub fn owner_confirms(call: &Call, what: &str) -> Result<(), CepError> {
    if call.principal.owner {
        Ok(())
    } else {
        call.require_approval(what)
    }
}

/// The summary of `flows.get`, `flows.list` and `flows.save`
pub fn summary(service: &Service, store: &Store, file: &FlowFile, warnings: Option<usize>) -> Value {
    let settings = store.settings(file.scope, &file.name);
    let mut out = json!({
        "name": file.name,
        "scope": file.scope.as_str(),
        "path": display(&file.path),
        "format": file.format.as_str(),
        "enabled": settings["enabled"].as_bool().unwrap_or(false),
        "updated_at": file.modified.map(time_text),
    });
    match &file.flow {
        Ok(flow) => {
            out["description"] = flow.get("description").cloned().unwrap_or(Value::Null);
            let enabled = out["enabled"].as_bool().unwrap_or(false);
            out["triggers"] = json!(service.triggers.statuses(&service.paths, file.scope, flow, enabled));
            out["hash"] = json!(cluaiz_automation::validate::hash(flow));
            out["tags"] = flow.get("meta").and_then(|m| m.get("tags")).cloned().unwrap_or(json!([]));
            out["warnings"] = json!(warnings);
        }
        Err(problem) => {
            out["enabled"] = json!(false);
            out["problems"] = json!([problem]);
        }
    }
    let last = journal::list_runs(&service.paths, Some(&file.name)).into_iter().find(|r| r.scope == file.scope.as_str());
    out["last_run"] = last.map(|r| json!({ "run_id": r.run_id, "status": r.status, "finished_at": r.finished_at })).unwrap_or(Value::Null);
    out
}
