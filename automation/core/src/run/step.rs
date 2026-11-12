//! One step, the same way for every kind (docs/automation/steps/README.md#how-one-step-runs):
//! `when` → `approve_before` → mocks → `idempotency_key` → attempts with `retry` and `timeout` →
//! `output` → record; on failure, `on_error`.

use super::kinds::KindOut;
use super::{state, RunState, Runner, Stop};
use crate::error::{FlowError, Kind, NEVER_RETRIED};
use crate::format::{self, generated_id, tables::KindDef};
use crate::journal::{self, Attempt, StepRecord};
use crate::units::parse_duration;
use crate::{limits, refs};
use futures::future::BoxFuture;
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

/// Checking for a cancel this often while a step waits on the Engine
const CANCEL_POLL: Duration = Duration::from_millis(100);
/// `secret_output` records show this instead of the value
const MASK: &str = "***";

/// Where a step is: its id, its place in the file, and the loop item it runs for
pub(super) struct At<'a> {
    pub id: &'a str,
    pub place: &'a str,
    pub item: &'a Value,
}

/// The item key of a step inside a loop inside another loop
pub(super) fn nested_item(outer: &Value, index: usize) -> Value {
    if outer.is_null() {
        json!({ "index": index })
    } else {
        json!({ "index": index, "in": outer })
    }
}

/// Waits for `work`, unless the run (or the branch it is in) is cancelled first
pub(super) async fn cancellable<T>(st: &RunState, work: impl std::future::Future<Output = T>) -> Option<T> {
    let stops = st.stops.clone();
    let watch = async move {
        while !stops.iter().any(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
            tokio::time::sleep(CANCEL_POLL).await;
        }
    };
    tokio::select! {
        done = work => Some(done),
        _ = watch => None,
    }
}

/// What each step of a list waits for: the step before it, or the steps in its `needs`
fn dependencies(steps: &[Value], ids: &[String]) -> Vec<Vec<usize>> {
    steps
        .iter()
        .enumerate()
        .map(|(i, step)| match step.get("needs").and_then(Value::as_array) {
            None => if i == 0 { vec![] } else { vec![i - 1] },
            Some(needs) => needs.iter().filter_map(Value::as_str).filter_map(|n| ids.iter().position(|id| id == n)).collect(),
        })
        .collect()
}

/// Steps whose dependencies have all finished; validation refuses cycles, and if one slips
/// through the first unfinished step runs alone
fn ready(deps: &[Vec<usize>], finished: &[bool]) -> Vec<usize> {
    let ready: Vec<usize> = (0..deps.len()).filter(|&i| !finished[i] && deps[i].iter().all(|&d| finished[d])).collect();
    if ready.is_empty() {
        return (0..deps.len()).find(|&i| !finished[i]).into_iter().collect();
    }
    ready
}

/// Kinds that do work on the Engine: they take one of the run's `max_parallel` slots
const WORKING_KINDS: [&str; 5] = ["tool", "call", "ask", "agent", "code"];
/// Kinds whose `timeout` is how long they wait, not how long one attempt may take
const WAITING_KINDS: [&str; 2] = ["wait", "approve"];

struct Retry {
    max: usize,
    delay_ms: i64,
    exponential: bool,
    max_delay_ms: i64,
    jitter: bool,
    on: Vec<String>,
    respect_retry_after: bool,
}

impl Retry {
    fn of(value: Option<&Value>) -> Option<Retry> {
        let value = value?;
        let duration = |field: &str, default: i64| value.get(field).and_then(Value::as_str).and_then(|t| parse_duration(t).ok()).unwrap_or(default);
        Some(Retry {
            max: value.get("max").and_then(Value::as_u64).unwrap_or(0).min(20) as usize,
            delay_ms: duration("delay", 5_000),
            exponential: value.get("backoff").and_then(Value::as_str) != Some("fixed"),
            max_delay_ms: duration("max_delay", 300_000),
            jitter: value.get("jitter").and_then(Value::as_bool).unwrap_or(true),
            on: value.get("on").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default(),
            respect_retry_after: value.get("respect_retry_after").and_then(Value::as_bool).unwrap_or(true),
        })
    }

    fn allows(&self, error: &FlowError, retries_done: usize) -> bool {
        if retries_done >= self.max || NEVER_RETRIED.contains(&error.kind.as_str()) {
            return false;
        }
        if self.on.is_empty() {
            return Kind::ALL.iter().any(|k| k.retried_by_default() && error.is(*k));
        }
        self.on.iter().any(|k| k == "any" || *k == error.kind)
    }

    fn wait_ms(&self, retry: usize, error: &FlowError) -> u64 {
        if self.respect_retry_after {
            if let Some(after) = error.details.get("retry_after_ms").and_then(Value::as_i64) {
                return after.clamp(0, self.max_delay_ms) as u64;
            }
        }
        let base = if self.exponential { self.delay_ms.saturating_mul(1 << retry.min(20)) } else { self.delay_ms };
        let mut wait = base.min(self.max_delay_ms).max(0) as f64;
        if self.jitter {
            // Up to 20% more, so many runs do not retry at the same moment
            let random = (uuid::Uuid::new_v4().as_u128() % 1_000) as f64 / 1_000.0;
            wait *= 1.0 + 0.2 * random;
        }
        wait as u64
    }
}

impl Runner {
    pub(super) fn run_list<'a>(&'a self, st: &'a mut RunState, steps: &'a [Value], parent: Option<&'a str>, part: &'a str, place: &'a str, item: &'a Value) -> BoxFuture<'a, Result<Value, Stop>> {
        Box::pin(async move {
            let ids: Vec<String> = steps.iter().enumerate().map(|(i, s)| s.get("id").and_then(Value::as_str).map(String::from).unwrap_or_else(|| generated_id(parent, part, i))).collect();
            let places: Vec<String> = (0..steps.len()).map(|i| format!("{place}[{i}]")).collect();
            let deps = dependencies(steps, &ids);
            let mut finished = vec![false; steps.len()];
            let mut outputs: Vec<Value> = vec![Value::Null; steps.len()];
            while finished.iter().any(|f| !f) {
                let wave = ready(&deps, &finished);
                if let [only] = wave.as_slice() {
                    let i = *only;
                    finished[i] = true;
                    let Some(map) = steps[i].as_object() else { continue };
                    outputs[i] = self.run_step(st, map, At { id: &ids[i], place: &places[i], item }).await?;
                    continue;
                }
                // Steps whose needs are all met go together, each on a copy of the context
                let mut going = futures::stream::FuturesUnordered::new();
                let mut stops = Vec::new();
                for &i in &wave {
                    finished[i] = true;
                    let Some(map) = steps[i].as_object() else { continue };
                    let (mut branch, stop) = st.fork();
                    stops.push(stop);
                    let (id, step_place) = (&ids[i], &places[i]);
                    going.push(async move {
                        let result = self.run_step(&mut branch, map, At { id, place: step_place, item }).await;
                        (i, branch, result)
                    });
                }
                let mut first_stop: Option<Stop> = None;
                while let Some((i, branch, result)) = futures::StreamExt::next(&mut going).await {
                    st.join(&branch);
                    if let Some(view) = branch.ctx.step_view(&ids[i]) {
                        st.ctx.set_step(&ids[i], view);
                    }
                    match result {
                        Ok(output) => outputs[i] = output,
                        Err(stop) => {
                            // The first failure (or return) stops the steps going with it
                            stops.iter().for_each(|s| s.store(true, std::sync::atomic::Ordering::Relaxed));
                            if first_stop.is_none() || matches!(first_stop, Some(Stop::Cancelled)) {
                                first_stop = Some(stop);
                            }
                        }
                    }
                }
                if let Some(stop) = first_stop {
                    return Err(stop);
                }
            }
            Ok(outputs.pop().unwrap_or(Value::Null))
        })
    }

    async fn run_step(&self, st: &mut RunState, map: &Map<String, Value>, at: At<'_>) -> Result<Value, Stop> {
        let Ok(kind) = format::step_kind(map) else {
            return Err(Stop::Fail(FlowError::new(Kind::InvalidInput, format!("the step at {} has no kind", at.place))));
        };
        if st.cancelled() {
            return Err(Stop::Cancelled);
        }
        let steps_run = st.run.steps_run.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if steps_run > st.run.limits.max_steps {
            return Err(Stop::Fail(FlowError::new(Kind::Limit, format!("more than limits.max_steps ({}) steps ran", st.run.limits.max_steps)).at_step(at.id)));
        }
        let key = journal::record_key(at.place, at.item);
        let reused = st.run.done.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
        // The same place holds the same step (a flow changed since may have moved steps)
        if let Some(done) = reused.filter(|d| d.id == at.id) {
            self.reuse(st, kind, &done);
            if st.run.copy_records {
                let copied = journal::StepRecord { status: "reused".into(), ..done.clone() };
                st.log("step_finished", serde_json::to_value(&copied).unwrap_or_default());
            }
            return Ok(done.output);
        }
        let mut record = StepRecord {
            id: at.id.to_string(),
            place: at.place.to_string(),
            kind: kind.key.to_string(),
            status: "running".into(),
            item: at.item.clone(),
            attempts: Vec::new(),
            input: Value::Null,
            output: Value::Null,
            error: None,
            tokens: Value::Null,
            model: None,
            detail: Value::Null,
        };
        st.log("step_started", json!({ "id": at.id, "place": at.place, "kind": kind.key, "item": at.item }));
        st.event("step.started", json!({ "id": at.id, "place": at.place, "kind": kind.key, "item": at.item }));
        let started = Instant::now();
        let started_at = journal::now_text();
        let body = self.step_body(st, map, kind, &at, &key, &mut record).await;
        let outcome = match body {
            Ok((status, output, extra)) => {
                record.status = status.into();
                record.output = output.clone();
                Ok((output, extra))
            }
            Err(Stop::Fail(error)) => {
                let error = error.at_step(at.id);
                record.status = "failed".into();
                record.error = Some(error.clone());
                self.on_error(st, map, &at, error).await.map(|output| {
                    record.output = output.clone();
                    (output, Map::new())
                })
            }
            Err(Stop::Return(value)) => {
                record.status = "succeeded".into();
                Err(Stop::Return(value))
            }
            Err(Stop::Cancelled) => {
                record.status = "cancelled".into();
                Err(Stop::Cancelled)
            }
        };
        let ms = started.elapsed().as_millis() as u64;
        let secret = map.get("secret_output") == Some(&Value::Bool(true));
        let mut step_view = json!({
            "output": record.output, "status": record.status, "error": record.error, "attempts": record.attempts.len().max(1),
            "started_at": started_at, "finished_at": journal::now_text(), "ms": ms, "tokens": record.tokens,
        });
        if let (Ok((_, extra)), Some(view)) = (&outcome, step_view.as_object_mut()) {
            view.extend(extra.clone());
        }
        st.ctx.set_step(at.id, step_view);
        if secret {
            record.output = json!(MASK);
        }
        st.log("step_finished", serde_json::to_value(&record).unwrap_or_default());
        st.event("step.finished", json!({ "id": at.id, "status": record.status, "ms": ms, "tokens": record.tokens }));
        st.update(|r| r.steps_done += 1);
        outcome.map(|(output, _)| output)
    }

    /// A step that finished before a restart: its output is used, and a `set` puts its values back
    fn reuse(&self, st: &mut RunState, kind: &KindDef, done: &StepRecord) {
        if kind.key == "set" {
            if let (Some(vars), Some(values)) = (st.ctx.vars.as_object_mut(), done.output.as_object()) {
                vars.extend(values.clone());
            }
        }
        st.ctx.set_step(&done.id, json!({ "output": done.output, "status": done.status, "error": done.error, "attempts": done.attempts.len().max(1), "tokens": done.tokens }));
    }

    /// The step's work; `Ok((status, output, fields added to steps.<id>))`
    async fn step_body(&self, st: &mut RunState, map: &Map<String, Value>, kind: &'static KindDef, at: &At<'_>, key: &str, record: &mut StepRecord) -> Result<(&'static str, Value, Map<String, Value>), Stop> {
        if let Some(when) = map.get("when") {
            if !st.ctx.decide(when).map_err(Stop::Fail)? {
                return Ok(("skipped", Value::Null, Map::new()));
            }
        }
        if let Some(question) = map.get("approve_before") {
            let question = refs::to_text(&st.ctx.resolve(question).map_err(Stop::Fail)?);
            let answer = self.ask_and_wait(st, &question, at, Value::Null, None).await?;
            if answer.approved != Some(true) {
                return Err(Stop::Fail(FlowError::new(Kind::ApprovalRejected, format!("the owner did not approve: {question}"))));
            }
        }
        if kind.key == "agent" && map.get("allow_untrusted") != Some(&Value::Bool(true)) && self.untrusted(st, map) {
            let question = format!("Step {} of flow {} is a model that reads outside content and can use tools that send data out. Let it run?", at.id, st.info.flow);
            let answer = self.ask_and_wait(st, &question, at, json!({ "tools": map.get("tools") }), None).await?;
            if answer.approved != Some(true) {
                return Err(Stop::Fail(FlowError::new(Kind::ApprovalRejected, "the owner did not let a model with outside content and sending tools run (untrusted_with_tools)")));
            }
        }
        if let Some(mock) = map.get("id").and_then(Value::as_str).and_then(|id| st.run.mocks.get(id)) {
            record.detail = json!({ "mocked": true });
            return Ok(("succeeded", mock.clone(), Map::new()));
        }
        let idempotency = match map.get("idempotency_key") {
            Some(k) => Some(refs::to_text(&st.ctx.resolve(k).map_err(Stop::Fail)?)),
            None => None,
        };
        if let Some(idem) = &idempotency {
            if let Some(output) = state::idempotent_output(&self.paths, &st.info.scope, &st.info.flow, idem) {
                record.detail = json!({ "idempotency_key": idem });
                return Ok(("reused", output, Map::new()));
            }
            if st.run.unfinished.contains(key) && map.get("on_unknown").and_then(Value::as_str) != Some("repeat") {
                let question = format!("Step {} of flow {} was running when Automation stopped, so it is not known whether it happened. Run it again?", at.id, st.info.flow);
                if !self.host.ask_owner(&st.info, &question, at.place).await.map_err(Stop::Fail)? {
                    record.detail = json!({ "idempotency_key": idem, "unknown": "the owner chose not to run it again" });
                    return Ok(("unknown", Value::Null, Map::new()));
                }
            }
        }
        let retry = Retry::of(map.get("retry"));
        let timeout = if WAITING_KINDS.contains(&kind.key) { None } else { limits::step_timeout_ms(kind.key, map.get("timeout")) };
        let out: KindOut = loop {
            let attempt_started = Instant::now();
            let attempt_at = journal::now_text();
            let shared = st.run.clone();
            let _slot = if WORKING_KINDS.contains(&kind.key) { shared.slots.acquire().await.ok() } else { None };
            let work = self.exec_kind(st, map, kind, at);
            let result = match timeout {
                Some(ms) => match tokio::time::timeout(Duration::from_millis(ms.max(0) as u64), work).await {
                    Ok(result) => result,
                    Err(_) => Err(Stop::Fail(FlowError::new(Kind::Timeout, format!("the step took longer than its timeout ({} ms)", ms)))),
                },
                None => work.await,
            };
            let error = match &result {
                Err(Stop::Fail(error)) => Some(error.clone()),
                _ => None,
            };
            record.attempts.push(Attempt { started_at: attempt_at, ms: attempt_started.elapsed().as_millis() as u64, error: error.clone() });
            match (result, &retry) {
                (Ok(out), _) => break out,
                (Err(Stop::Fail(error)), Some(retry)) if retry.allows(&error, record.attempts.len() - 1) => {
                    let wait = retry.wait_ms(record.attempts.len() - 1, &error);
                    st.event("step.retrying", json!({ "id": at.id, "attempt": record.attempts.len(), "error": error, "wait_ms": wait }));
                    // A retry delay is waiting: it counts toward max_wait, not timeout
                    st.run.clock.wait_starts();
                    let slept = cancellable(st, tokio::time::sleep(Duration::from_millis(wait))).await;
                    st.run.clock.wait_ends();
                    if slept.is_none() {
                        return Err(Stop::Cancelled);
                    }
                }
                (Err(stop), _) => return Err(stop),
            }
        };
        record.input = out.input.clone();
        record.tokens = out.tokens.clone();
        record.model = out.model.clone();
        if !out.detail.is_null() {
            record.detail = out.detail.clone();
        }
        let mut output = match map.get("output") {
            Some(shape) => {
                st.ctx.push_local("result", out.result);
                let shaped = st.ctx.resolve(shape);
                st.ctx.pop_locals(1);
                shaped.map_err(Stop::Fail)?
            }
            None => out.result,
        };
        output = self.keep_large(st, at, output);
        if let Some(idem) = &idempotency {
            if !st.run.dry_run {
                state::keep_idempotent(&self.paths, &st.info.scope, &st.info.flow, idem, &output);
            }
        }
        Ok((out.status, output, out.extra))
    }

    /// A value larger than `limits.result_in_memory` is written to the run's files and passed on
    /// as a file reference
    pub(super) fn keep_large(&self, st: &RunState, at: &At<'_>, value: Value) -> Value {
        let text = value.to_string();
        if (text.len() as u64) <= st.run.limits.result_in_memory || refs::is_file_ref(&value) {
            return value;
        }
        let item = at.item.get("index").map(|i| format!(".{i}")).unwrap_or_default();
        let name = format!("{}{item}.output.json", at.id.replace(['/', '\\'], "_"));
        let path = st.run.dir.files().join(&name);
        match std::fs::write(&path, text.as_bytes()) {
            Ok(()) => {
                let relative = format!("runs/{}/{}/files/{name}", st.info.flow, st.info.run_id);
                refs::file_ref(&relative, text.len() as u64, "application/json")
            }
            Err(error) => {
                tracing::warn!("Automation kept a large value in memory, the file could not be written: {error}");
                value
            }
        }
    }

    /// On a failed step: `stop` (default) fails the run, `continue` goes on, steps handle it
    async fn on_error(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>, error: FlowError) -> Result<Value, Stop> {
        match map.get("on_error") {
            Some(Value::String(mode)) if mode == "continue" => Ok(Value::Null),
            Some(Value::Array(handlers)) => {
                st.ctx.push_local("error", error.to_value());
                st.ctx.push_scope();
                let handler_place = format!("{}.on_error", at.place);
                let handled = self.run_list(st, handlers, Some(at.id), "on_error", &handler_place, at.item).await;
                st.ctx.pop_scope();
                st.ctx.pop_locals(1);
                handled
            }
            _ => Err(Stop::Fail(error)),
        }
    }
}
