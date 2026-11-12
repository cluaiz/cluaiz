//! `agent`: a model that calls tools for several rounds before it answers
//! (docs/automation/steps/agent.md). Automation runs the rounds itself: the model gets the tools'
//! schemas, and every call it makes is run as the flow (its permissions apply), recorded, and
//! given back to it. At `max_turns` or when `stop_when` holds, it is asked for its final answer.

use super::kinds::{check_answer, tool_allowed, tool_output, tools_call_params, KindOut, STEP_INSTRUCTIONS};
use super::step::cancellable;
use super::{RunState, Runner, Stop};
use crate::error::{FlowError, Kind};
use crate::host::{AskAnswer, AskRequest, NodeInfo};
use crate::validate::uses::{untrusted_with_tools, ToolName};
use crate::{permissions, refs};
use serde_json::{json, Map, Value};
use std::time::Instant;

const DEFAULT_TURNS: u64 = 10;
const MAX_TURNS: u64 = 50;
/// A tool's answer is shortened to this many characters before the model reads it
const MAX_RESULT_CHARS: usize = 20_000;

/// A function name the model can call: OpenAI names allow letters, digits, `_` and `-`
fn function_name(tool: &str, taken: &[String]) -> String {
    let mut name: String = tool.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    name.truncate(60);
    let base = name.clone();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

fn shorten(text: String) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let kept: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{kept}\n…(shortened: the result was longer than {MAX_RESULT_CHARS} characters)")
}

/// The tools an agent step may call, as the model sees them
struct Toolbox {
    /// (function name, tool name, its parts)
    tools: Vec<(String, String, ToolName)>,
    schemas: Vec<Value>,
}

impl Runner {
    /// The outside-content rule for this step, with what is installed now
    pub(super) fn untrusted(&self, st: &RunState, map: &Map<String, Value>) -> bool {
        let infos: Vec<Option<NodeInfo>> = map.get("tools").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(|t| self.host.catalog().tool(t)).collect();
        !infos.is_empty() && untrusted_with_tools(st.run.outside_content, &infos)
    }

    fn toolbox(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<Toolbox, Stop> {
        let mut toolbox = Toolbox { tools: Vec::new(), schemas: Vec::new() };
        for tool in map.get("tools").and_then(Value::as_array).into_iter().flatten() {
            let name = refs::to_text(&st.ctx.resolve(tool).map_err(Stop::Fail)?);
            let parts = tool_allowed(&st.info.permissions, &name).map_err(Stop::Fail)?;
            let taken: Vec<String> = toolbox.tools.iter().map(|(f, _, _)| f.clone()).collect();
            let function = function_name(&name, &taken);
            let info = self.host.catalog().tool(&name).unwrap_or_default();
            let parameters = info.input_schema.unwrap_or_else(|| json!({ "type": "object" }));
            let description = info.description.unwrap_or_else(|| format!("The tool {name}"));
            toolbox.schemas.push(json!({ "type": "function", "function": { "name": function, "description": description, "parameters": parameters } }));
            toolbox.tools.push((function, name, parts));
        }
        Ok(toolbox)
    }

    pub(super) async fn agent(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        for field in ["attachments", "session"] {
            if map.get(field).is_some_and(|v| !v.is_null()) {
                return Err(Stop::Fail(FlowError::not_built(&format!("`{field}` on an agent step"))));
            }
        }
        if map.get("tool_search") == Some(&Value::Bool(true)) {
            return Err(Stop::Fail(FlowError::not_built("`tool_search` on an agent step")));
        }
        let text = |st: &mut RunState, field: &str| -> Result<Option<String>, Stop> { map.get(field).map(|v| st.ctx.resolve(v).map(|v| refs::to_text(&v))).transpose().map_err(Stop::Fail) };
        let instruction = text(st, "agent")?.unwrap_or_default();
        let model = text(st, "model")?;
        if !permissions::allows_model(&st.info.permissions, model.as_deref()) {
            return Err(Stop::Fail(FlowError::new(Kind::Permission, format!("the model `{}` is not in the flow's permissions.models", model.as_deref().unwrap_or("default (needs *)")))));
        }
        let data = match map.get("with") {
            Some(with) => st.ctx.resolve(with).map_err(Stop::Fail)?,
            None => Value::Null,
        };
        let toolbox = self.toolbox(st, map)?;
        let system = match text(st, "system")? {
            Some(extra) => format!("{STEP_INSTRUCTIONS} Use the tools when they help; answer when you have what you need.\n\n{extra}"),
            None => format!("{STEP_INSTRUCTIONS} Use the tools when they help; answer when you have what you need."),
        };
        let mut user = instruction.clone();
        match &data {
            Value::Null => {}
            Value::String(text) => user.push_str(&format!("\n\nData:\n{text}")),
            other => user.push_str(&format!("\n\nData:\n{other}")),
        }
        let output_schema = map.get("output_schema").cloned();
        let max_turns = map.get("max_turns").and_then(Value::as_u64).unwrap_or(DEFAULT_TURNS).clamp(1, MAX_TURNS);
        let base = AskRequest {
            model: model.clone(),
            think: map.get("think").and_then(Value::as_str).map(String::from),
            max_tokens: map.get("max_tokens").and_then(Value::as_u64),
            ..Default::default()
        };
        let mut messages = vec![json!({ "role": "system", "content": system }), json!({ "role": "user", "content": user })];
        let mut rounds: Vec<Value> = Vec::new();
        let (mut prompt_tokens, mut completion_tokens) = (0u64, 0u64);
        let mut used_model = String::new();
        let mut last: Option<AskAnswer> = None;
        for turn in 0..max_turns {
            let request = AskRequest { messages: messages.clone(), tools: toolbox.schemas.clone(), ..base.clone() };
            let info = st.info.clone();
            let answer = cancellable(st, self.host.ask(&info, request)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
            prompt_tokens += answer.prompt_tokens;
            completion_tokens += answer.completion_tokens;
            used_model = answer.model.clone();
            if answer.tool_calls.is_empty() {
                last = Some(answer);
                break;
            }
            messages.push(json!({ "role": "assistant", "content": answer.text, "tool_calls": answer.tool_calls }));
            let mut calls = Vec::new();
            for call in &answer.tool_calls {
                let function = call["function"]["name"].as_str().unwrap_or_default();
                let arguments = call["function"]["arguments"].as_str().map(|a| serde_json::from_str(a).unwrap_or(Value::Null)).unwrap_or_else(|| call["function"]["arguments"].clone());
                let started = Instant::now();
                let (tool, result) = match toolbox.tools.iter().find(|(f, _, _)| f == function) {
                    None => (function.to_string(), Err(FlowError::new(Kind::NotFound, format!("there is no tool {function}; use one of the tools given")))),
                    Some((_, name, parts)) => {
                        let input = if arguments.is_object() { arguments.clone() } else { json!({}) };
                        let info = st.info.clone();
                        let raw = cancellable(st, self.host.call(&info, "tools.call", tools_call_params(parts, input))).await.ok_or(Stop::Cancelled)?;
                        (name.clone(), raw.and_then(|raw| tool_output(&parts.kind, raw)))
                    }
                };
                // A failed call goes back to the model as a result; it may try another way
                let content = match &result {
                    Ok(value) => shorten(refs::to_text(value)),
                    Err(error) => format!("error ({}): {}", error.kind, error.message),
                };
                messages.push(json!({ "role": "tool", "tool_call_id": call["id"], "content": content }));
                calls.push(json!({ "tool": tool, "input": arguments, "ok": result.is_ok(), "error": result.as_ref().err(), "ms": started.elapsed().as_millis() as u64 }));
            }
            let round = json!({ "index": turn, "calls": calls });
            rounds.push(round.clone());
            if let Some(stop_when) = map.get("stop_when") {
                st.ctx.push_local("round", round);
                let stop = st.ctx.decide(stop_when);
                st.ctx.pop_locals(1);
                if stop.map_err(Stop::Fail)? {
                    break;
                }
            }
        }
        // No answer yet (max_turns or stop_when): one last turn without tools, for the answer
        let answer = match last {
            Some(answer) => answer,
            None => {
                messages.push(json!({ "role": "user", "content": "Give your final answer now, without calling tools." }));
                let request = AskRequest { messages: messages.clone(), output_schema: output_schema.clone(), ..base.clone() };
                let info = st.info.clone();
                let answer = cancellable(st, self.host.ask(&info, request)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
                prompt_tokens += answer.prompt_tokens;
                completion_tokens += answer.completion_tokens;
                if answer.text.trim().is_empty() {
                    return Err(Stop::Fail(FlowError::new(Kind::Limit, "max_turns reached and the model gave no answer when asked").with_details(json!({ "rounds": rounds }))));
                }
                answer
            }
        };
        let result = match &output_schema {
            None => Value::String(answer.text.trim().to_string()),
            Some(schema) => match check_answer(&answer.text, schema) {
                Ok(value) => value,
                Err(_) => {
                    // Once more, with the answer's shape fixed by the schema
                    messages.push(json!({ "role": "assistant", "content": answer.text }));
                    messages.push(json!({ "role": "user", "content": "Answer again with JSON only, in the required shape." }));
                    let request = AskRequest { messages: messages.clone(), output_schema: Some(schema.clone()), ..base.clone() };
                    let info = st.info.clone();
                    let again = cancellable(st, self.host.ask(&info, request)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
                    prompt_tokens += again.prompt_tokens;
                    completion_tokens += again.completion_tokens;
                    check_answer(&again.text, schema).map_err(|problems| Stop::Fail(FlowError::new(Kind::InvalidOutput, format!("the answer did not match output_schema: {}", problems.join("; "))).with_details(json!({ "answer": again.text }))))?
                }
            },
        };
        let mut out = KindOut::new(result, json!({ "agent": instruction, "with": data, "model": model, "tools": toolbox.tools.iter().map(|(_, n, _)| n.clone()).collect::<Vec<_>>() }));
        out.tokens = json!({ "prompt": prompt_tokens, "completion": completion_tokens });
        out.model = Some(used_model);
        out.detail = json!({ "rounds": rounds, "thinking": answer.thinking });
        out.extra.insert("rounds".into(), json!(rounds.len()));
        Ok(out)
    }
}
