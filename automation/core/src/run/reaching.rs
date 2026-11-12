//! Steps that reach beyond the run: `emit` (an event for other flows), `run` (another flow) and
//! `code` (a WebAssembly module). The host does the work; these check and shape it
//! (docs/automation/steps/emit-return-fail.md, run.md, code.md).

use super::kinds::KindOut;
use super::step::cancellable;
use super::{RunState, Runner, Stop};
use crate::error::{FlowError, Kind};
use crate::host::{CodeCall, FlowCall};
use crate::refs;
use crate::units::parse_size;
use serde_json::{json, Map, Value};

/// Event data at most (*proposed*: 1 MB)
const MAX_EVENT_BYTES: usize = 1 << 20;
/// Flows started by `run` steps inside each other, at most (*proposed*)
pub const MAX_DEPTH: u64 = 8;
/// A module's memory, at most and by default
const MAX_MEMORY: u64 = 256 << 20;
/// Instructions a module may run by default
const DEFAULT_FUEL: u64 = 2_000_000_000;

fn fail(kind: Kind, message: impl Into<String>) -> Stop {
    Stop::Fail(FlowError::new(kind, message))
}

/// `with`, worked out; `default` when absent
fn input(st: &mut RunState, map: &Map<String, Value>, default: Value) -> Result<Value, Stop> {
    match map.get("with") {
        Some(with) => st.ctx.resolve(with).map_err(Stop::Fail),
        None => Ok(default),
    }
}

/// Letters, digits, `.`, `-`, `_`; `engine.` is the Engine's own
pub fn event_name_problem(name: &str) -> Option<String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        return Some(format!("`{name}` is not an event name: use letters, digits, `.`, `-`, `_`"));
    }
    name.starts_with("engine.").then(|| format!("`{name}`: names starting with engine. are the Engine's own"))
}

impl Runner {
    pub(super) async fn emit(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        let name = refs::to_text(&st.ctx.resolve(&map["emit"]).map_err(Stop::Fail)?);
        if let Some(problem) = event_name_problem(&name) {
            return Err(fail(Kind::InvalidInput, problem));
        }
        let data = input(st, map, Value::Null)?;
        if data.to_string().len() > MAX_EVENT_BYTES {
            return Err(fail(Kind::Limit, "an event's data is at most 1 MB; send a file reference instead"));
        }
        // Events that start flows that send events: a chain stops at MAX_DEPTH, so two flows that
        // start each other cannot go on for ever
        let hops = st.ctx.trigger.get("hops").and_then(Value::as_u64).unwrap_or(0) + 1;
        if hops > MAX_DEPTH {
            return Err(fail(Kind::Limit, format!("this event would be the {hops}th in a chain of flows starting each other (at most {MAX_DEPTH})")));
        }
        let record_input = json!({ "emit": name, "with": data });
        if st.run.dry_run {
            let mut out = KindOut::new(json!({ "name": name, "delivered": 0 }), record_input);
            out.detail = json!({ "dry_run": "not sent" });
            return Ok(out);
        }
        let info = st.info.clone();
        let delivered = cancellable(st, self.host.emit(&info, &name, data, hops)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
        Ok(KindOut::new(json!({ "name": name, "delivered": delivered }), record_input))
    }

    pub(super) async fn run_flow(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        let name = refs::to_text(&st.ctx.resolve(&map["run"]).map_err(Stop::Fail)?);
        let inputs = input(st, map, json!({}))?;
        let depth = st.ctx.trigger.get("parent").and_then(|p| p.get("depth")).and_then(Value::as_u64).unwrap_or(0) + 1;
        if depth > MAX_DEPTH {
            return Err(fail(Kind::Limit, format!("flows started by run steps go {MAX_DEPTH} deep at most")));
        }
        let call = FlowCall {
            name: name.clone(),
            scope: map.get("scope").and_then(Value::as_str).map(String::from),
            inputs: inputs.clone(),
            wait: map.get("wait").and_then(Value::as_bool).unwrap_or(true),
            parent: json!({ "flow": st.info.flow, "scope": st.info.scope, "run_id": st.info.run_id, "depth": depth }),
            dry_run: st.run.dry_run,
        };
        let info = st.info.clone();
        let output = cancellable(st, self.host.run_flow(&info, call)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
        Ok(KindOut::new(output, json!({ "run": name, "with": inputs })))
    }

    pub(super) async fn code(&self, st: &mut RunState, map: &Map<String, Value>) -> Result<KindOut, Stop> {
        let module = refs::to_text(&st.ctx.resolve(&map["code"]).map_err(Stop::Fail)?);
        let function = map.get("fn").and_then(Value::as_str).unwrap_or("main").to_string();
        let data = input(st, map, Value::Null)?;
        let memory_bytes = match map.get("memory").and_then(Value::as_str) {
            Some(text) => parse_size(text).map_err(|m| fail(Kind::InvalidInput, format!("`memory`: {m}")))?.min(MAX_MEMORY),
            None => MAX_MEMORY,
        };
        let fuel = map.get("fuel").and_then(Value::as_u64).unwrap_or(DEFAULT_FUEL).max(1);
        let network: Vec<String> = st.info.permissions.get("network").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
        let record_input = json!({ "code": module, "fn": function, "with": data });
        // A module that can reach the network may change something; one that cannot only computes
        if st.run.dry_run && !network.is_empty() {
            let mut out = KindOut::new(Value::Null, record_input);
            out.detail = json!({ "dry_run": "not run: the module may reach the network" });
            return Ok(out);
        }
        let call = CodeCall { module, function, input: data, memory_bytes, fuel, network };
        let info = st.info.clone();
        let answer = cancellable(st, self.host.code(&info, call)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?;
        if let Some(error) = answer.as_object().filter(|m| m.len() == 1).and_then(|m| m.get("error")) {
            let message = error.get("message").and_then(Value::as_str).map(String::from).unwrap_or_else(|| refs::to_text(error));
            return Err(Stop::Fail(FlowError::new(Kind::Code, message).with_details(json!({ "error": error }))));
        }
        Ok(KindOut::new(answer, record_input))
    }
}
