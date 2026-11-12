//! Steps that run other steps together or again: `parallel`, `repeat`, and the items of `each`
//! with `parallel` (docs/automation/steps/parallel.md, repeat.md, each.md). A branch runs on a copy
//! of the context, so it sees the steps before it and its own, not another branch's.

use super::kinds::KindOut;
use super::step::{cancellable, nested_item, At};
use super::{RunState, Runner, Stop};
use crate::error::{FlowError, Kind};
use crate::units::parse_duration;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::{json, Map, Value};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Most rounds of one `repeat`
const MAX_ROUNDS: u64 = 10_000;

/// One list of steps to run beside others
pub(super) struct Branch {
    pub steps: Vec<Value>,
    /// For generated ids: `parallel.news`, `do`
    pub part: String,
    /// For records: `steps[2].parallel.news`
    pub place: String,
    /// The record key's item (`each` items, `null` for `parallel` branches)
    pub item: Value,
    /// Values the branch's steps read by name (`item`, `index`)
    pub locals: Vec<(String, Value)>,
}

/// How a group of branches goes together
#[derive(Clone, Copy)]
pub(super) struct Together {
    /// Branches at the same time
    pub width: usize,
    /// The first failure stops the others
    pub stop_on_error: bool,
    pub ends: Ends,
    /// Branch `n` starts no sooner than `n × delay_ms` after the first
    pub delay_ms: u64,
}

/// When a group of branches is done
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Ends {
    /// Every branch has ended (or one failed, with `stop_on_error`)
    All,
    /// The first branch that succeeds ends the others (`wait: any`)
    FirstSuccess,
}

impl Runner {
    /// Runs branches together, as `how` says. A result is `None` for a branch that never
    /// started, or that was stopped because the step ended.
    pub(super) async fn run_branches(&self, st: &mut RunState, at: &At<'_>, branches: Vec<Branch>, how: Together) -> Vec<Option<Result<Value, Stop>>> {
        let Together { width, stop_on_error, ends, delay_ms } = how;
        let count = branches.len();
        let mut results: Vec<Option<Result<Value, Stop>>> = (0..count).map(|_| None).collect();
        let mut stops = Vec::with_capacity(count);
        let mut going = FuturesUnordered::new();
        let mut next = 0;
        let mut ended = false;
        let first_start = Instant::now();
        let parent = at.id;
        loop {
            while !ended && going.len() < width.max(1) && next < count {
                let index = next;
                next += 1;
                let (mut branch, stop) = st.fork();
                stops.push(stop);
                let plan = &branches[index];
                let start_at = first_start + Duration::from_millis(delay_ms.saturating_mul(index as u64));
                going.push(async move {
                    let wait = start_at.saturating_duration_since(Instant::now());
                    if !wait.is_zero() && cancellable(&branch, tokio::time::sleep(wait)).await.is_none() {
                        return (index, branch, Err(Stop::Cancelled));
                    }
                    for (name, value) in &plan.locals {
                        branch.ctx.push_local(name, value.clone());
                    }
                    branch.ctx.push_scope();
                    let result = self.run_list(&mut branch, &plan.steps, Some(parent), &plan.part, &plan.place, &plan.item).await;
                    (index, branch, result)
                });
            }
            let Some((index, branch, result)) = going.next().await else { break };
            st.join(&branch);
            let ends_all = match &result {
                Ok(_) => ends == Ends::FirstSuccess,
                Err(Stop::Fail(_)) => stop_on_error,
                Err(_) => true,
            };
            results[index] = Some(result);
            if ends_all && !ended {
                ended = true;
                stops.iter().for_each(|s| s.store(true, Ordering::Relaxed));
            }
        }
        results
    }

    pub(super) async fn parallel(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let named = map.get("parallel").and_then(Value::as_object).cloned().unwrap_or_default();
        let any = map.get("wait").and_then(Value::as_str) == Some("any");
        let stop_on_error = map.get("stop_on_error").and_then(Value::as_bool).unwrap_or(true);
        let names: Vec<String> = named.keys().cloned().collect();
        let branches = named
            .iter()
            .map(|(name, steps)| Branch {
                steps: steps.as_array().cloned().unwrap_or_default(),
                part: format!("parallel.{name}"),
                place: format!("{}.parallel.{name}", at.place),
                item: at.item.clone(),
                locals: Vec::new(),
            })
            .collect();
        let ends = if any { Ends::FirstSuccess } else { Ends::All };
        let results = self.run_branches(st, at, branches, Together { width: names.len(), stop_on_error, ends, delay_ms: 0 }).await;
        let mut output = Map::new();
        let mut failed = Vec::new();
        let mut winner = None;
        let mut first_error = None;
        for (name, result) in names.iter().zip(results) {
            match result {
                Some(Ok(value)) => {
                    if any && winner.is_none() {
                        winner = Some(name.clone());
                        output.insert(name.clone(), value);
                    } else if !any {
                        output.insert(name.clone(), value);
                    }
                }
                Some(Err(Stop::Fail(error))) => {
                    failed.push(json!({ "branch": name, "error": error }));
                    first_error.get_or_insert(error);
                    if !any {
                        output.insert(name.clone(), Value::Null);
                    }
                }
                Some(Err(Stop::Return(value))) => return Err(Stop::Return(value)),
                Some(Err(Stop::Cancelled)) | None => {
                    if st.cancelled() {
                        return Err(Stop::Cancelled);
                    }
                }
            }
        }
        if any && winner.is_none() {
            let error = first_error.unwrap_or_else(|| FlowError::new(Kind::Failed, "no branch succeeded"));
            return Err(Stop::Fail(FlowError { kind: error.kind.clone(), message: format!("no branch succeeded: {}", error.message), step: None, details: json!({ "failed": failed }) }));
        }
        if stop_on_error && !any {
            if let Some(error) = first_error {
                return Err(Stop::Fail(error.with_details(json!({ "failed": failed }))));
            }
        }
        let mut out = KindOut::new(Value::Object(output), json!({ "branches": names, "wait": if any { "any" } else { "all" } }));
        if let Some(name) = winner {
            out.extra.insert("winner".into(), json!(name));
        }
        if !failed.is_empty() {
            out.extra.insert("failed".into(), Value::Array(failed));
        }
        Ok(out)
    }

    pub(super) async fn repeat(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let steps = map.get("repeat").and_then(Value::as_array).cloned().unwrap_or_default();
        let max = map.get("max").and_then(Value::as_u64).unwrap_or(1).clamp(1, MAX_ROUNDS);
        let delay_ms = map.get("delay").and_then(Value::as_str).and_then(|d| parse_duration(d).ok()).unwrap_or(0).max(0) as u64;
        let place = format!("{}.repeat", at.place);
        let mut previous = Value::Null;
        for round in 0..max {
            if round > 0 && delay_ms > 0 {
                let waited = self.waiting(st, json!({ "kind": "delay", "step": at.id }), tokio::time::sleep(Duration::from_millis(delay_ms))).await;
                if waited.is_none() {
                    return Err(Stop::Cancelled);
                }
            }
            // `steps.<repeat id>.previous` inside the round
            st.ctx.set_step(at.id, json!({ "previous": previous, "round": round }));
            st.ctx.push_local("round", json!(round));
            st.ctx.push_scope();
            let item = nested_item(at.item, round as usize);
            let result = self.run_list(st, &steps, Some(at.id), "repeat", &place, &item).await;
            let done = match &result {
                Ok(_) => st.ctx.decide(&map["until"]),
                Err(_) => Ok(false),
            };
            st.ctx.pop_scope();
            st.ctx.pop_locals(1);
            previous = result?;
            if done.map_err(Stop::Fail)? {
                let mut out = KindOut::new(previous, json!({ "max": max }));
                out.extra.insert("rounds".into(), json!(round + 1));
                return Ok(out);
            }
        }
        if map.get("on_max").and_then(Value::as_str) == Some("continue") {
            let mut out = KindOut::new(previous, json!({ "max": max }));
            out.extra.insert("rounds".into(), json!(max));
            out.detail = json!({ "until": "never held; on_max: continue" });
            return Ok(out);
        }
        Err(Stop::Fail(FlowError::new(Kind::Limit, format!("`until` did not hold after {max} rounds (max)"))))
    }
}
