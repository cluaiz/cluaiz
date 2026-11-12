//! `flows.nodes` and `flows.explain` (docs/automation/methods/).

use super::common::{self, given, FormatName, Given, ScopeName};
use super::drafting;
use crate::service;
use cluaiz_api::cep::CallCtx;
use cluaiz_automation::ease::{self, explain};
use cluaiz_automation::validate;
use cluaiz_cep::{Call, CepError, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Tool,
    Call,
    All,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesParams {
    /// Only tools, only methods, or both (default)
    pub kind: Option<NodeKind>,
    /// Words matched against names and descriptions; every word must match
    pub query: Option<String>,
    /// Include input and output schemas (default true); false makes a short list
    pub schemas: Option<bool>,
    /// Include `steps`, `triggers` and `operators` (default true)
    pub catalog: Option<bool>,
}

pub async fn nodes(ctx: CallCtx, _call: Call, params: NodesParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let mut nodes = service.snapshot().await.nodes();
    match params.kind.unwrap_or(NodeKind::All) {
        NodeKind::Tool => nodes.retain(|n| n["step"] == "tool"),
        NodeKind::Call => nodes.retain(|n| n["step"] == "call"),
        NodeKind::All => {}
    }
    if let Some(query) = params.query.as_deref().map(str::to_lowercase) {
        let words: Vec<&str> = query.split_whitespace().collect();
        nodes.retain(|n| {
            let text = format!("{} {} {}", n["ref"].as_str().unwrap_or_default(), n["title"].as_str().unwrap_or_default(), n["description"].as_str().unwrap_or_default()).to_lowercase();
            words.iter().all(|w| text.contains(w))
        });
    }
    if !params.schemas.unwrap_or(true) {
        for node in &mut nodes {
            if let Some(map) = node.as_object_mut() {
                map.remove("input_schema");
                map.remove("output_schema");
            }
        }
    }
    let mut out = json!({ "nodes": nodes });
    if params.catalog.unwrap_or(true) {
        out["steps"] = json!(ease::step_kinds());
        out["triggers"] = json!(ease::trigger_kinds());
        out["operators"] = json!(ease::operators());
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExplainMode {
    Plain,
    Model,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExplainParams {
    /// A saved flow
    pub name: Option<String>,
    /// Which flow of that name. Absent: the project's, then the user's
    pub scope: Option<ScopeName>,
    /// The flow as JSON. Send `name`, `flow`, or `text` with `format`
    pub flow: Option<Value>,
    /// The flow as JSON or YAML text
    pub text: Option<String>,
    /// The format of `text`
    pub format: Option<FormatName>,
    /// plain (default): made from the flow, no model; model: a model writes it more naturally
    pub mode: Option<ExplainMode>,
    /// For `mode: model`: the language to write in (default English)
    pub language: Option<String>,
    /// For `mode: model`: the model (default the default chat model)
    pub model: Option<String>,
}

pub async fn explain(ctx: CallCtx, call: Call, params: ExplainParams) -> Result<Value, CepError> {
    let service = service::get(&ctx.state);
    let mode = params.mode.unwrap_or(ExplainMode::Plain);
    if mode == ExplainMode::Model {
        // A model runs: the caller needs exec too
        call.principal.allow(Scope::Exec, false)?;
    }
    let (flow, expected) = match params.name {
        Some(name) => {
            let store = service.store().await;
            let file = common::find(&store, &name, common::scope_of(params.scope))?;
            let flow = file.flow.clone().map_err(|problem| common::invalid_flow(&[problem]))?;
            (flow, Some(file.name))
        }
        None => match given(params.flow, params.text, params.format)? {
            Given::Parsed { flow, .. } => (flow, None),
            Given::Broken(problem) => return Err(common::invalid_flow(&[problem])),
        },
    };
    let catalog = service.snapshot().await;
    let report = validate::validate(&flow, expected.as_deref(), &catalog);
    // A node that is not installed is said in the warnings; other errors make the flow unreadable
    if report.problems.iter().any(|p| p.is_error() && p.code != "unknown_node") {
        return Err(common::invalid_flow(&report.problems));
    }
    let mut plain = explain::explain(&flow, &report, &catalog);
    if let Some(list) = plain["warnings"].as_array_mut() {
        for problem in report.problems.iter().filter(|p| p.code == "unknown_node") {
            list.push(json!(format!("{}: {}", problem.path, problem.message)));
        }
    }
    if mode == ExplainMode::Plain {
        return Ok(plain);
    }
    let language = params.language.unwrap_or_else(|| "English".into());
    let door = crate::engine::EngineDoor { state: ctx.state.clone(), catalog };
    let mut talk = drafting::Talk::new(&door, params.model);
    let (out, model) = drafting::rewrite(&mut talk, plain, drafting::Language::Named(&language)).await.map_err(drafting::model_error)?;
    let mut out = out;
    out["model"] = json!(model);
    Ok(out)
}
