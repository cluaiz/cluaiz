//! What each kind of step does (docs/automation/steps/). Kinds that reach the Engine go through
//! the [`StepHost`](crate::host::StepHost); the others work on the run's context.

use super::flowing::{Branch, Ends, Together};
use super::step::{cancellable, nested_item, At};
use super::{RunState, Runner, Stop};
use crate::error::{FlowError, Kind};
use crate::expr::values::equal;
use crate::format::tables::KindDef;
use crate::host::AskRequest;
use crate::validate::uses::{parse_tool, ToolName};
use crate::{permissions, refs, schema};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// The system text every `ask` step starts with (docs/automation/steps/ask.md#what-the-model-receives)
pub(super) const STEP_INSTRUCTIONS: &str = "You are one step of an automated flow; answer only what is asked.";

/// What a kind gives back to the step around it
pub(super) struct KindOut {
    pub status: &'static str,
    pub result: Value,
    /// The step's input after references, for the record
    pub input: Value,
    pub tokens: Value,
    pub model: Option<String>,
    pub detail: Value,
    /// Fields added to `steps.<id>` (`branch`, `failed`, …)
    pub extra: Map<String, Value>,
}

impl KindOut {
    pub(super) fn new(result: Value, input: Value) -> Self {
        Self { status: "succeeded", result, input, tokens: Value::Null, model: None, detail: Value::Null, extra: Map::new() }
    }
}

fn fail(kind: Kind, message: impl Into<String>) -> Stop {
    Stop::Fail(FlowError::new(kind, message))
}

/// A file reference inside an input becomes the file's full path: tools and methods take paths
fn paths_for_files(st: &RunState, value: Value) -> Value {
    match value {
        Value::Object(_) if refs::is_file_ref(&value) => st.ctx.file_path(&value).map(|p| Value::String(p.display().to_string())).unwrap_or(value),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, paths_for_files(st, v))).collect()),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| paths_for_files(st, v)).collect()),
        other => other,
    }
}

/// A tool the flow may use, split into the parts `tools.call` takes
pub(crate) fn tool_allowed(permissions: &Value, name: &str) -> Result<ToolName, FlowError> {
    let tool = parse_tool(name).map_err(|m| FlowError::new(Kind::InvalidInput, m))?;
    if !permissions::allows_tool(permissions, name) {
        return Err(FlowError::new(Kind::Permission, format!("`{name}` is not in the flow's permissions.tools")));
    }
    Ok(tool)
}

pub(crate) fn tools_call_params(tool: &ToolName, input: Value) -> Value {
    json!({ "kind": tool.kind, "name": tool.name, "fn": tool.function, "input": input })
}

/// A method the flow may call
pub(crate) fn method_allowed(permissions: &Value, method: &str) -> Result<(), FlowError> {
    if permissions::not_callable(method) {
        return Err(FlowError::new(Kind::NotCallable, format!("`{method}` cannot be called from a flow")));
    }
    if permissions::ROUTES.contains(&method) {
        return Err(FlowError::not_built(&format!("the Engine route `{method}` in a call step")));
    }
    if !permissions::allows_method(permissions, method) {
        return Err(FlowError::new(Kind::Permission, format!("`{method}` is not in the flow's permissions.methods")));
    }
    Ok(())
}

impl Runner {
    pub(super) async fn exec_kind(&self, st: &mut RunState, map: &Map<String, Value>, kind: &'static KindDef, at: &At<'_>) -> Result<KindOut, Stop> {
        match kind.key {
            "tool" => self.tool(st, map).await,
            "call" => self.call(st, map).await,
            "ask" => self.ask(st, map).await,
            "if" => {
                let yes = st.ctx.decide(&map["if"]).map_err(Stop::Fail)?;
                let branch = if yes { "then" } else { "else" };
                let steps = map.get(branch).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                let output = self.branch(st, steps, at, branch, &format!("{}.{branch}", at.place), at.item).await?;
                let mut out = KindOut::new(output, json!({ "if": yes }));
                out.extra.insert("branch".into(), json!(branch));
                Ok(out)
            }
            "switch" => self.switch(st, map, at).await,
            "each" => self.each(st, map, at).await,
            "set" => {
                let values = st.ctx.resolve(&map["set"]).map_err(Stop::Fail)?;
                if let (Some(vars), Some(new)) = (st.ctx.vars.as_object_mut(), values.as_object()) {
                    vars.extend(new.clone());
                }
                Ok(KindOut::new(values.clone(), values))
            }
            "return" => Err(Stop::Return(st.ctx.resolve(&map["return"]).map_err(Stop::Fail)?)),
            "fail" => {
                let message = refs::to_text(&st.ctx.resolve(&map["fail"]).map_err(Stop::Fail)?);
                let error_kind = map.get("kind").and_then(Value::as_str).unwrap_or("failed").to_string();
                let details = match map.get("details") {
                    Some(details) => st.ctx.resolve(details).map_err(Stop::Fail)?,
                    None => Value::Null,
                };
                Err(Stop::Fail(FlowError { kind: error_kind, message, step: None, details }))
            }
            "parallel" => self.parallel(st, map, at).await,
            "repeat" => self.repeat(st, map, at).await,
            "wait" => self.wait(st, map, at).await,
            "approve" => self.approve(st, map, at).await,
            "emit" => self.emit(st, map).await,
            "run" => self.run_flow(st, map).await,
            "code" => self.code(st, map).await,
            "agent" => self.agent(st, map).await,
            other => Err(Stop::Fail(FlowError::not_built(&format!("the `{other}` step")))),
        }
    }

    /// The steps of a branch, in a scope of their own; the output of the last one
    async fn branch(&self, st: &mut RunState, steps: &[Value], at: &At<'_>, part: &str, place: &str, item: &Value) -> Result<Value, Stop> {
        st.ctx.push_scope();
        let result = self.run_list(st, steps, Some(at.id), part, place, item).await;
        st.ctx.pop_scope();
        result
    }

    async fn tool(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        let name = refs::to_text(&st.ctx.resolve(&map["tool"]).map_err(Stop::Fail)?);
        let tool = tool_allowed(&st.info.permissions, &name).map_err(Stop::Fail)?;
        if map.get("settings").is_some_and(|s| s.as_object().is_some_and(|m| !m.is_empty())) {
            return Err(Stop::Fail(FlowError::not_built("`settings` on a tool step")));
        }
        let input = match map.get("with") {
            Some(with) => st.ctx.resolve(with).map_err(Stop::Fail)?,
            None => json!({}),
        };
        let record_input = json!({ "tool": name, "with": input });
        let info = self.host.catalog().tool(&name);
        if let Some(schema_value) = info.as_ref().and_then(|i| i.input_schema.as_ref()) {
            let errors = schema::validate(&input, schema_value);
            if !errors.is_empty() {
                let list: Vec<Value> = errors.iter().map(|e| e.to_value()).collect();
                return Err(Stop::Fail(FlowError::new(Kind::InvalidInput, format!("the input does not match {name}: {}", errors.iter().map(|e| format!("with{} {}", e.path.replace('/', "."), e.message)).collect::<Vec<_>>().join("; "))).with_details(json!({ "errors": list }))));
            }
        }
        if st.run.dry_run && !info.as_ref().is_some_and(|i| i.read_only) {
            let mut out = KindOut::new(Value::Null, record_input);
            out.detail = json!({ "dry_run": "not run: it may change something" });
            return Ok(out);
        }
        let params = tools_call_params(&tool, paths_for_files(st, input));
        let info_run = st.info.clone();
        let raw = cancellable(st, self.host.call(&info_run, "tools.call", params)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
        let result = tool_output(&tool.kind, raw).map_err(Stop::Fail)?;
        Ok(KindOut::new(result, record_input))
    }

    async fn call(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        let method = refs::to_text(&st.ctx.resolve(&map["call"]).map_err(Stop::Fail)?);
        method_allowed(&st.info.permissions, &method).map_err(Stop::Fail)?;
        let params = match map.get("with") {
            Some(with) => st.ctx.resolve(with).map_err(Stop::Fail)?,
            None => json!({}),
        };
        if !params.is_object() {
            return Err(fail(Kind::InvalidInput, format!("a call step's `with` is an object of {method}'s params")));
        }
        let record_input = json!({ "call": method, "with": params });
        let read_only = self.host.catalog().method(&method).is_some_and(|i| i.read_only);
        if st.run.dry_run && !read_only {
            let mut out = KindOut::new(Value::Null, record_input);
            out.detail = json!({ "dry_run": "not run: it may change something" });
            return Ok(out);
        }
        let info_run = st.info.clone();
        let result = cancellable(st, self.host.call(&info_run, &method, paths_for_files(st, params))).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
        Ok(KindOut::new(result, record_input))
    }

    async fn ask(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        for field in ["tools", "attachments", "session"] {
            if map.get(field).is_some_and(|v| !v.is_null() && v != &json!([])) {
                return Err(Stop::Fail(FlowError::not_built(&format!("`{field}` on an ask step"))));
            }
        }
        let text_of = |st: &mut RunState, field: &str| -> Result<Option<String>, Stop> {
            map.get(field).map(|v| st.ctx.resolve(v).map(|v| refs::to_text(&v))).transpose().map_err(Stop::Fail)
        };
        let instruction = text_of(st, "ask")?.unwrap_or_default();
        let model = text_of(st, "model")?;
        if !permissions::allows_model(&st.info.permissions, model.as_deref()) {
            return Err(fail(Kind::Permission, format!("the model `{}` is not in the flow's permissions.models", model.as_deref().unwrap_or("default (needs *)"))));
        }
        let data = match map.get("with") {
            Some(with) => st.ctx.resolve(with).map_err(Stop::Fail)?,
            None => Value::Null,
        };
        let data = load_files(st, data);
        let mut user = instruction.clone();
        match &data {
            Value::Null => {}
            Value::String(text) => user.push_str(&format!("\n\nData:\n{text}")),
            other => user.push_str(&format!("\n\nData:\n{other}")),
        }
        let system = match text_of(st, "system")? {
            Some(extra) => format!("{STEP_INSTRUCTIONS}\n\n{extra}"),
            None => STEP_INSTRUCTIONS.to_string(),
        };
        let output_schema = map.get("output_schema").cloned();
        let number = |field: &str| map.get(field).and_then(Value::as_f64);
        let mut request = AskRequest {
            model: model.clone(),
            system,
            user: user.clone(),
            output_schema: output_schema.clone(),
            think: map.get("think").and_then(Value::as_str).map(String::from),
            effort: map.get("effort").and_then(Value::as_str).map(String::from),
            max_tokens: map.get("max_tokens").and_then(Value::as_u64),
            temperature: number("temperature"),
            top_p: number("top_p"),
            seed: map.get("seed").and_then(Value::as_i64),
            ..Default::default()
        };
        let record_input = json!({ "ask": instruction, "with": data, "model": model });
        let retries = map.get("schema_retries").and_then(Value::as_u64).unwrap_or(1);
        let (mut prompt_tokens, mut completion_tokens) = (0u64, 0u64);
        let mut tries = 0;
        loop {
            let info_run = st.info.clone();
            let answer = cancellable(st, self.host.ask(&info_run, request.clone())).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
            prompt_tokens += answer.prompt_tokens;
            completion_tokens += answer.completion_tokens;
            let finish = |result: Value| {
                let mut out = KindOut::new(result, record_input.clone());
                out.tokens = json!({ "prompt": prompt_tokens, "completion": completion_tokens });
                out.model = Some(answer.model.clone());
                out.detail = json!({ "thinking": answer.thinking, "schema_attempts": tries + 1 });
                out
            };
            let Some(wanted) = &output_schema else {
                return Ok(finish(Value::String(answer.text.trim().to_string())));
            };
            let problems = match check_answer(&answer.text, wanted) {
                Ok(parsed) => return Ok(finish(parsed)),
                Err(problems) => problems,
            };
            if tries >= retries {
                return Err(Stop::Fail(FlowError::new(Kind::InvalidOutput, format!("the answer did not match output_schema: {}", problems.join("; "))).with_details(json!({ "answer": answer.text, "problems": problems }))));
            }
            tries += 1;
            request.user = format!("{user}\n\nYour last answer did not have the required JSON shape: {}. Answer again with JSON only.", problems.join("; "));
        }
    }

    async fn switch(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let matched = match map.get("match") {
            Some(value) => Some(st.ctx.resolve(value).map_err(Stop::Fail)?),
            None => None,
        };
        let cases = map.get("switch").and_then(Value::as_array).cloned().unwrap_or_default();
        for (i, case) in cases.iter().enumerate() {
            let holds = match (&matched, case.get("is"), case.get("when")) {
                (Some(value), Some(Value::Array(options)), _) => options.iter().any(|o| equal(o, value)),
                (Some(value), Some(option), _) => equal(option, value),
                (None, _, Some(when)) => st.ctx.decide(when).map_err(Stop::Fail)?,
                _ => false,
            };
            if holds {
                let steps = case.get("do").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                let output = self.branch(st, steps, at, &format!("switch.{i}.do"), &format!("{}.switch[{i}].do", at.place), at.item).await?;
                let mut out = KindOut::new(output, json!({ "case": i, "match": matched }));
                out.extra.insert("case".into(), json!(case.get("name").cloned().unwrap_or(json!(i))));
                return Ok(out);
            }
        }
        let steps = map.get("default").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let output = self.branch(st, steps, at, "default", &format!("{}.default", at.place), at.item).await?;
        let mut out = KindOut::new(output, json!({ "case": "default", "match": matched }));
        out.extra.insert("case".into(), json!("default"));
        Ok(out)
    }

    async fn each(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let resolved = st.ctx.resolve(&map["each"]).map_err(Stop::Fail)?;
        let items = match load_files(st, resolved) {
            Value::Array(items) => items,
            Value::Object(fields) => fields.into_iter().map(|(key, value)| json!({ "key": key, "value": value })).collect(),
            Value::Null => Vec::new(),
            other => return Err(fail(Kind::Expression, format!("each needs a list or an object, not {other}"))),
        };
        let name = map.get("as").and_then(Value::as_str).unwrap_or("item").to_string();
        let batch = map.get("batch").and_then(Value::as_u64).filter(|b| *b > 0).map(|b| b as usize);
        let stop_on_error = map.get("stop_on_error").and_then(Value::as_bool).unwrap_or(true);
        let delay = map.get("delay").and_then(Value::as_str).and_then(|d| crate::units::parse_duration(d).ok()).unwrap_or(0).max(0) as u64;
        let steps = map.get("do").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut groups: Vec<Value> = match batch {
            Some(size) => items.chunks(size).map(|c| Value::Array(c.to_vec())).collect(),
            None => items,
        };
        let total = groups.len();
        let mut outputs = Vec::with_capacity(total);
        let mut failed = Vec::new();
        let place = format!("{}.do", at.place);
        let width = map.get("parallel").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
        if width > 1 {
            // Items at the same time, each on a copy of the context
            let locals_name = if batch.is_some() { "items".to_string() } else { name.clone() };
            let branches = std::mem::take(&mut groups)
                .into_iter()
                .enumerate()
                .map(|(index, group)| Branch { steps: steps.clone(), part: "do".into(), place: place.clone(), item: nested_item(at.item, index), locals: vec![(locals_name.clone(), group), ("index".into(), json!(index))] })
                .collect();
            let results = self.run_branches(st, at, branches, Together { width, stop_on_error, ends: Ends::All, delay_ms: delay }).await;
            let mut first_error = None;
            for (index, result) in results.into_iter().enumerate() {
                match result {
                    Some(Ok(output)) => outputs.push(output),
                    Some(Err(Stop::Fail(error))) => {
                        failed.push(json!({ "index": index, "error": error }));
                        first_error.get_or_insert(error);
                        outputs.push(Value::Null);
                    }
                    Some(Err(Stop::Return(value))) => return Err(Stop::Return(value)),
                    Some(Err(Stop::Cancelled)) | None => {
                        if st.cancelled() {
                            return Err(Stop::Cancelled);
                        }
                        outputs.push(Value::Null);
                    }
                }
            }
            if let (true, Some(error)) = (stop_on_error, first_error) {
                return Err(Stop::Fail(error));
            }
        }
        let sequential = if width > 1 { Vec::new() } else { std::mem::take(&mut groups) };
        for (index, group) in sequential.into_iter().enumerate() {
            if index > 0 && delay > 0 && cancellable(st, tokio::time::sleep(Duration::from_millis(delay))).await.is_none() {
                return Err(Stop::Cancelled);
            }
            let pushed = if batch.is_some() {
                st.ctx.push_local("items", group);
                st.ctx.push_local("index", json!(index));
                2
            } else {
                st.ctx.push_local(&name, group);
                st.ctx.push_local("index", json!(index));
                2
            };
            let item = nested_item(at.item, index);
            let result = self.branch(st, &steps, at, "do", &place, &item).await;
            st.ctx.pop_locals(pushed);
            match result {
                Ok(output) => outputs.push(output),
                Err(Stop::Fail(error)) if !stop_on_error => {
                    failed.push(json!({ "index": index, "error": error }));
                    outputs.push(Value::Null);
                }
                Err(other) => return Err(other),
            }
        }
        let mut out = KindOut::new(Value::Array(outputs), json!({ "items": total, "as": name }));
        if !stop_on_error {
            out.extra.insert("succeeded_count".into(), json!(total - failed.len()));
            out.extra.insert("failed_count".into(), json!(failed.len()));
            out.extra.insert("failed".into(), Value::Array(failed));
        }
        Ok(out)
    }
}

/// A file reference whose content a step needs as a value (the list of `each`, the data of
/// `ask`): read from the run's files
fn load_files(st: &RunState, value: Value) -> Value {
    if !refs::is_file_ref(&value) {
        return value;
    }
    let Some(path) = st.ctx.file_path(&value) else { return value };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
        Err(_) => value,
    }
}

/// A model's answer read as JSON and checked against `output_schema`: the value, or what is wrong
pub(super) fn check_answer(text: &str, wanted: &Value) -> Result<Value, Vec<String>> {
    let parsed = parse_json_answer(text).map_err(|problem| vec![problem])?;
    let errors = schema::validate(&parsed, wanted);
    if errors.is_empty() {
        return Ok(parsed);
    }
    Err(errors.iter().map(|e| format!("{} {}", if e.path.is_empty() { "the answer" } else { &e.path }, e.message)).collect())
}

/// A model's JSON answer; a fenced block (```json … ```) is opened first
fn parse_json_answer(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.trim_end().strip_suffix("```"))
        .unwrap_or(trimmed);
    serde_json::from_str(inner.trim()).map_err(|e| format!("the answer is not valid JSON ({e})"))
}

/// What a tool's answer becomes as the step's output (docs/automation/steps/tool.md#output)
pub(crate) fn tool_output(kind: &str, raw: Value) -> Result<Value, FlowError> {
    if kind == "skill" {
        let text = refs::to_text(&raw);
        return Ok(skill_output(&text));
    }
    match raw {
        Value::Object(map) if map.get("isError") == Some(&Value::Bool(true)) => {
            let message = content_text(map.get("content")).unwrap_or_else(|| "the tool reported an error".into());
            Err(FlowError::new(Kind::Tool, message))
        }
        Value::Object(map) if map.contains_key("structuredContent") => Ok(map["structuredContent"].clone()),
        Value::Object(map) if map.get("content").is_some_and(Value::is_array) => {
            let text = content_text(map.get("content")).unwrap_or_default();
            Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
        }
        Value::String(text) => Ok(serde_json::from_str(&text).unwrap_or(Value::String(text))),
        other => Ok(other),
    }
}

fn content_text(content: Option<&Value>) -> Option<String> {
    let items = content?.as_array()?;
    let texts: Vec<&str> = items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("text")).filter_map(|i| i.get("text").and_then(Value::as_str)).collect();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

/// A skill's `SKILL.md`: `{ name, description, text }` from its front matter and body
fn skill_output(text: &str) -> Value {
    let (front, body) = match text.strip_prefix("---") {
        Some(rest) => match rest.split_once("\n---") {
            Some((front, body)) => (front, body.trim_start_matches(['-', '\n', '\r'])),
            None => ("", text),
        },
        None => ("", text),
    };
    let field = |name: &str| front.lines().find_map(|l| l.trim().strip_prefix(&format!("{name}:")).map(|v| v.trim().trim_matches('"').to_string()));
    json!({ "name": field("name"), "description": field("description"), "text": body.trim() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_answers() {
        assert_eq!(tool_output("mcp", json!({ "structuredContent": { "a": 1 } })).unwrap(), json!({ "a": 1 }));
        assert_eq!(tool_output("mcp", json!({ "content": [{ "type": "text", "text": "{\"n\":2}" }] })).unwrap(), json!({ "n": 2 }));
        assert_eq!(tool_output("mcp", json!({ "content": [{ "type": "text", "text": "a" }, { "type": "text", "text": "b" }] })).unwrap(), json!("a\nb"));
        assert_eq!(tool_output("mcp", json!({ "isError": true, "content": [{ "type": "text", "text": "no repo" }] })).unwrap_err().kind, "tool");
        assert_eq!(tool_output("plugin", json!("[1,2]")).unwrap(), json!([1, 2]));
        assert_eq!(tool_output("skill", json!("---\nname: review\ndescription: Reviews code\n---\nRead the diff.")).unwrap(), json!({ "name": "review", "description": "Reviews code", "text": "Read the diff." }));
    }

    #[test]
    fn json_answers() {
        assert_eq!(parse_json_answer("```json\n{\"kind\": \"bug\"}\n```").unwrap(), json!({ "kind": "bug" }));
        assert!(parse_json_answer("It is a bug").is_err());
    }
}
