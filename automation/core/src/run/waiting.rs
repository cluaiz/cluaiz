//! Steps that wait: `wait`, `approve`, and the owner's approval before a step
//! (docs/automation/steps/wait.md, approve.md). While a step waits, the run is `waiting` in its
//! record. Its deadline is written to the journal, so after a restart it waits only for what was
//! left. Waiting counts toward `limits.max_wait`, not `limits.timeout`.

use super::kinds::{tool_allowed, tools_call_params, KindOut};
use super::step::{cancellable, At};
use super::{RunState, Runner, Stop};
use crate::error::{FlowError, Kind};
use crate::expr::values::parse_time;
use crate::host::{OwnerAnswer, OwnerQuestion};
use crate::journal::{self, RunStatus};
use crate::refs;
use crate::units::parse_duration;
use chrono::Utc;
use serde_json::{json, Map, Value};
use std::time::Duration;

/// Sleeps are cut into pieces this long, and the wall clock read again (a computer that slept)
const NAP_MS: i64 = 30_000;

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn time_text(ms: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(ms).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)).unwrap_or_default()
}

async fn sleep_until(until_ms: i64) {
    loop {
        let left = until_ms - now_ms();
        if left <= 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(left.min(NAP_MS) as u64)).await;
    }
}

fn duration(map: &Map<String, Value>, field: &str) -> Result<Option<i64>, Stop> {
    match map.get(field).and_then(Value::as_str) {
        Some(text) => parse_duration(text).map(Some).map_err(|m| Stop::Fail(FlowError::new(Kind::InvalidInput, format!("`{field}`: {m}")))),
        None => Ok(None),
    }
}

impl Runner {
    /// Marks the run `waiting` while `work` goes, and counts that time as waiting
    pub(super) async fn waiting<T>(&self, st: &RunState, what: Value, work: impl std::future::Future<Output = T>) -> Option<T> {
        st.run.clock.wait_starts();
        st.update(|r| {
            r.status = RunStatus::Waiting;
            r.waiting_for = what.clone();
        });
        st.event("run.waiting", json!({ "waiting_for": what }));
        let out = cancellable(st, work).await;
        st.run.clock.wait_ends();
        if !st.run.clock.waiting() {
            st.update(|r| {
                r.status = RunStatus::Running;
                r.waiting_for = Value::Null;
            });
        }
        st.event("run.resumed", json!({ "after": what.get("kind") }));
        out
    }

    /// Waiting time `limits.max_wait` still allows
    fn wait_budget(st: &RunState) -> i64 {
        (st.run.limits.max_wait_ms - st.run.clock.waited_ms() as i64).max(0)
    }

    /// When a wait of this step ends: the deadline written before a restart, or `ms` from now
    /// (then written to the journal)
    fn deadline(&self, st: &RunState, at: &At<'_>, part: &str, ms: i64) -> i64 {
        let place = format!("{}#{part}", at.place);
        let key = journal::record_key(&place, at.item);
        if let Some(until) = st.run.waits.get(&key).and_then(|w| w["until_ms"].as_i64()) {
            return until;
        }
        let until = now_ms().saturating_add(ms.max(0));
        st.log("step_waiting", json!({ "place": place, "item": at.item, "until_ms": until }));
        until
    }

    /// A question to the owner that waits up to `timeout_ms` (default: what `max_wait` allows)
    pub(super) async fn ask_and_wait(&self, st: &RunState, question: &str, at: &At<'_>, details: Value, timeout_ms: Option<i64>) -> Result<OwnerAnswer, Stop> {
        let budget = Self::wait_budget(st);
        let until = self.deadline(st, at, "approval", timeout_ms.unwrap_or(budget).min(budget));
        let left = until - now_ms();
        if left <= 0 {
            return Ok(OwnerAnswer::default());
        }
        let asked = OwnerQuestion { question: question.to_string(), target: format!("{} {}", st.info.flow, at.place), details, timeout_ms: Some(left as u64) };
        let what = json!({ "kind": "approval", "step": at.id, "question": question, "until": time_text(until) });
        let info = st.info.clone();
        self.waiting(st, what, self.host.approval(&info, asked)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)
    }

    pub(super) async fn approve(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let question = refs::to_text(&st.ctx.resolve(&map["approve"]).map_err(Stop::Fail)?);
        let shown = match map.get("with") {
            Some(with) => st.ctx.resolve(with).map_err(Stop::Fail)?,
            None => Value::Null,
        };
        let input = json!({ "approve": question, "with": shown });
        if st.run.dry_run {
            let mut out = KindOut::new(json!({ "approved": true, "by": "dry_run", "note": null, "at": journal::now_text() }), input);
            out.detail = json!({ "dry_run": "not asked: a dry run counts it as approved" });
            return Ok(out);
        }
        let mut detail = Value::Null;
        if let Some(tool) = map.get("notify") {
            let name = refs::to_text(&st.ctx.resolve(tool).map_err(Stop::Fail)?);
            let tool = tool_allowed(&st.info.permissions, &name).map_err(Stop::Fail)?;
            let message = json!({ "question": question, "with": shown, "flow": st.info.flow, "run_id": st.info.run_id });
            if let Err(error) = self.host.call(&st.info, "tools.call", tools_call_params(&tool, message)).await {
                // The question stays open in the Engine's approvals even when the message did not go
                detail = json!({ "notify_failed": error });
            }
        }
        let answer = self.ask_and_wait(st, &question, at, shown, duration(map, "timeout")?).await?;
        let (approved, by) = match answer.approved {
            Some(yes) => (yes, "owner"),
            None => match map.get("on_timeout").and_then(Value::as_str) {
                Some("approve") => (true, "timeout"),
                Some("fail") => return Err(Stop::Fail(FlowError::new(Kind::Timeout, format!("nobody answered in time: {question}")))),
                _ => (false, "timeout"),
            },
        };
        let output = json!({ "approved": approved, "by": by, "note": answer.note, "at": journal::now_text() });
        if !approved && map.get("on_reject").and_then(Value::as_str) != Some("continue") {
            let error = FlowError::new(Kind::ApprovalRejected, format!("not approved ({by}): {question}")).with_details(output);
            return Err(Stop::Fail(error));
        }
        let mut out = KindOut::new(output, input);
        out.detail = detail;
        Ok(out)
    }

    pub(super) async fn wait(&self, st: &mut RunState, map: &Map<String, Value>, at: &At<'_>) -> Result<KindOut, Stop> {
        let form = st.ctx.resolve(&map["wait"]).map_err(Stop::Fail)?;
        let form = form.as_object().cloned().unwrap_or_default();
        let budget = Self::wait_budget(st);
        let continue_on_timeout = map.get("on_timeout").and_then(Value::as_str) == Some("continue");
        let started = now_ms();
        let too_long = || Stop::Fail(FlowError::new(Kind::Limit, "the wait is longer than limits.max_wait allows"));
        // A time or a duration
        let fixed = match (duration(&form, "for")?, form.get("until").and_then(Value::as_str)) {
            (Some(ms), _) => Some(ms),
            (None, Some(text)) => {
                let time = parse_time(text).ok_or_else(|| Stop::Fail(FlowError::new(Kind::InvalidInput, format!("`until`: `{text}` is not a time"))))?;
                Some((time.timestamp_millis() - now_ms()).max(0))
            }
            _ => None,
        };
        if let Some(ms) = fixed {
            if ms > budget {
                return Err(too_long());
            }
            let until = self.deadline(st, at, "wait", ms);
            let what = json!({ "kind": "time", "step": at.id, "until": time_text(until) });
            self.waiting(st, what, sleep_until(until)).await.ok_or(Stop::Cancelled)?;
            return Ok(KindOut::new(json!({ "waited_ms": (now_ms() - started).max(0) }), Value::Object(form)));
        }
        let timeout = duration(map, "timeout")?.unwrap_or(budget).min(budget);
        let until = self.deadline(st, at, "wait", timeout);
        let left = (until - now_ms()).max(0) as u64;
        let got = if let Some(name) = form.get("event").and_then(Value::as_str) {
            let what = json!({ "kind": "event", "step": at.id, "event": name, "until": time_text(until) });
            let filter = form.get("filter").cloned();
            let mut events = self.host.events().ok_or_else(|| Stop::Fail(FlowError::new(Kind::Unavailable, "events are not available here")))?;
            let run_id = st.info.run_id.clone();
            let mut ctx = st.ctx.clone();
            let wanted = async move {
                loop {
                    let event = match events.recv().await {
                        Ok(event) => event,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return Ok(None),
                    };
                    // A run does not receive its own events
                    if event.name != name || event.from.get("run_id").and_then(Value::as_str) == Some(run_id.as_str()) {
                        continue;
                    }
                    let seen = json!({ "name": event.name, "data": event.data, "from": event.from });
                    let matches = match &filter {
                        Some(filter) => {
                            ctx.push_local("event", seen.clone());
                            let yes = ctx.decide(filter);
                            ctx.pop_locals(1);
                            yes?
                        }
                        None => true,
                    };
                    if matches {
                        return Ok::<Option<Value>, FlowError>(Some(seen));
                    }
                }
            };
            let timed = async move { tokio::time::timeout(Duration::from_millis(left), wanted).await.unwrap_or(Ok(None)) };
            self.waiting(st, what, timed).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?
        } else if form.get("webhook").and_then(Value::as_bool) == Some(true) {
            let what = json!({ "kind": "webhook", "step": at.id, "until": time_text(until) });
            let info = st.info.clone();
            let token = st.run.hook_token.clone();
            self.waiting(st, what, self.host.wait_webhook(&info, &token, left)).await.ok_or(Stop::Cancelled)?.map_err(Stop::Fail)?
        } else {
            return Err(Stop::Fail(FlowError::new(Kind::InvalidInput, "a wait has one of for, until, event or webhook")));
        };
        match got {
            Some(output) => Ok(KindOut::new(output, Value::Object(form))),
            None if continue_on_timeout => {
                let mut out = KindOut::new(Value::Null, Value::Object(form));
                out.detail = json!({ "timeout": "nothing came in time; on_timeout: continue" });
                Ok(out)
            }
            None => Err(Stop::Fail(FlowError::new(Kind::Timeout, "nothing came before the wait's timeout"))),
        }
    }
}
