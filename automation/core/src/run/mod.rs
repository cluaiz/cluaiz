//! The runner: one run from its first step to its outputs (docs/automation/README.md#how-a-run-goes).
//! Every step's start and end are written to the run's journal before the next step, so a run
//! that stops (a crash, a restart) goes on from where it was, without running finished steps again.

mod context;
mod agent;
mod flowing;
pub(crate) mod kinds;
mod reaching;
mod waiting;
pub mod state;
mod step;

pub use context::Context;

use crate::error::{FlowError, Kind};
use crate::expr::values::Zone;
use crate::host::{RunInfo, StepHost};
use crate::journal::{self, Paths, RunDir, RunRecord, RunStatus, StepRecord};
use crate::limits::Limits;
use crate::schema;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How a list of steps ended early
#[derive(Debug)]
pub enum Stop {
    /// A step failed and its `on_error` did not handle it
    Fail(FlowError),
    /// A `return` step: the run ends with these outputs
    Return(Value),
    Cancelled,
}

/// Reported to the run's task as it goes (`tasks.get` with `after`)
pub type Events = Arc<dyn Fn(Value) + Send + Sync>;

pub struct RunRequest {
    /// The flow, already validated
    pub flow: Value,
    pub scope: String,
    /// The permissions the owner confirmed
    pub permissions: Value,
    pub inputs: Value,
    /// The whole trigger data, with `kind` (and `id`, `by`…)
    pub trigger: Value,
    pub dry_run: bool,
    /// Step id → output to use instead of running that step
    pub mocks: Map<String, Value>,
    pub run_id: String,
    pub task_id: Option<String>,
    pub parent: Value,
    pub rerun_of: Value,
    /// The folder of the flow's file: `code` modules are found there
    pub flow_dir: Option<std::path::PathBuf>,
}

/// A run written to disk, ready to start
pub struct Created {
    dir: RunDir,
    record: RunRecord,
    request: RunRequest,
    resume: Option<journal::Journal>,
    /// The records in `resume` come from another run (`flows.rerun_from`): copied into this one
    copy: bool,
}

impl Created {
    pub fn run_id(&self) -> &str {
        &self.record.run_id
    }
    pub fn record(&self) -> &RunRecord {
        &self.record
    }
    /// The CEP task that shows the run
    pub fn set_task_id(&mut self, task_id: &str) {
        self.record.task_id = Some(task_id.to_string());
        let _ = self.dir.save(&self.record);
    }
}

/// What every branch of one run shares
pub(crate) struct Shared {
    pub dir: RunDir,
    pub record: Mutex<RunRecord>,
    pub limits: Limits,
    pub dry_run: bool,
    pub mocks: Map<String, Value>,
    /// Finished step records of the run before a restart, by place and item
    pub done: Mutex<HashMap<String, StepRecord>>,
    /// Steps that had started and not finished when the run stopped
    pub unfinished: HashSet<String>,
    /// What waiting steps wrote before a restart (their deadline), by place and item
    pub waits: HashMap<String, Value>,
    pub steps_run: AtomicU64,
    pub events: Events,
    pub clock: RunClock,
    /// Steps that do work (tool, call, ask, agent, code) at the same time: `limits.max_parallel`
    pub slots: tokio::sync::Semaphore,
    /// The proof of the run's own webhook address (`run.webhook_url`)
    pub hook_token: String,
    /// The flow's triggers bring outside content (the outside-content rule of `agent` steps)
    pub outside_content: bool,
    /// Reused records come from an earlier run and are written to this run as `reused`
    pub copy_records: bool,
}

/// Running time without waiting time: `limits.timeout` counts the first, `limits.max_wait` the second
pub(crate) struct RunClock {
    started: Instant,
    /// (waits going now, since when, waited so far in ms)
    waits: Mutex<(usize, Option<Instant>, u64)>,
}

impl RunClock {
    fn new() -> Self {
        Self { started: Instant::now(), waits: Mutex::new((0, None, 0)) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (usize, Option<Instant>, u64)> {
        self.waits.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn wait_starts(&self) {
        let mut w = self.lock();
        if w.0 == 0 {
            w.1 = Some(Instant::now());
        }
        w.0 += 1;
    }

    pub fn wait_ends(&self) {
        let mut w = self.lock();
        w.0 = w.0.saturating_sub(1);
        if w.0 == 0 {
            if let Some(since) = w.1.take() {
                w.2 += since.elapsed().as_millis() as u64;
            }
        }
    }

    /// Waited so far, a wait going now included
    pub fn waited_ms(&self) -> u64 {
        let w = self.lock();
        w.2 + w.1.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0)
    }

    pub fn waiting(&self) -> bool {
        self.lock().0 > 0
    }

    pub fn running_ms(&self) -> u64 {
        (self.started.elapsed().as_millis() as u64).saturating_sub(self.waited_ms())
    }
}

/// What one list of steps holds while it goes: the run's shared part, and its own context. A
/// branch of `parallel` (or an item of `each` with `parallel`) gets a copy of the context.
pub(crate) struct RunState {
    pub info: RunInfo,
    pub ctx: Context,
    pub run: Arc<Shared>,
    /// The run's cancel, and the cancels of the branches this one is inside
    pub stops: Vec<Arc<AtomicBool>>,
}

impl RunState {
    pub fn log(&self, event: &str, data: Value) {
        if let Err(error) = self.run.dir.append(event, data) {
            tracing::error!("Automation could not write the journal of {}: {error}", self.info.run_id);
        }
    }

    pub fn event(&self, name: &str, data: Value) {
        let mut event = json!({ "event": name, "data": data });
        journal::mask(&mut event, self.run.dir.secrets());
        (self.run.events)(event);
    }

    pub fn cancelled(&self) -> bool {
        self.stops.iter().any(|s| s.load(Ordering::Relaxed))
    }

    /// Writes `run.json` again
    pub fn save(&self) {
        let record = self.run.record.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.run.dir.save(&record);
    }

    pub fn update(&self, change: impl FnOnce(&mut RunRecord)) {
        let mut record = self.run.record.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut record);
        let _ = self.run.dir.save(&record);
    }

    /// A branch: the same run, a copy of the context, and a cancel of its own
    pub fn fork(&self) -> (RunState, Arc<AtomicBool>) {
        let stop = Arc::new(AtomicBool::new(false));
        let mut stops = self.stops.clone();
        stops.push(stop.clone());
        (RunState { info: self.info.clone(), ctx: self.ctx.clone(), run: self.run.clone(), stops }, stop)
    }

    /// What a finished branch leaves for the steps after it: `vars` it set, and paths
    /// `changed`/`previous` read
    pub fn join(&mut self, branch: &RunState) {
        if let (Some(vars), Some(theirs)) = (self.ctx.vars.as_object_mut(), branch.ctx.vars.as_object()) {
            vars.extend(theirs.clone());
        }
        for watched in &branch.ctx.watched {
            if !self.ctx.watched.contains(watched) {
                self.ctx.watched.push(watched.clone());
            }
        }
    }
}

pub struct Runner {
    pub host: Arc<dyn StepHost>,
    pub paths: Paths,
}

fn trigger_summary(trigger: &Value) -> Value {
    json!({ "id": trigger.get("id").cloned().unwrap_or(Value::Null), "kind": trigger.get("kind").cloned().unwrap_or(json!("manual")), "at": trigger.get("at").cloned().unwrap_or_else(|| json!(journal::now_text())) })
}

impl Runner {
    pub fn new(host: Arc<dyn StepHost>, paths: Paths) -> Self {
        Self { host, paths }
    }

    /// Writes the run's folder with status `queued`
    pub fn create(&self, request: RunRequest) -> Result<Created, FlowError> {
        let flow_name = request.flow.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut trigger = request.trigger.clone();
        if let Some(map) = trigger.as_object_mut() {
            map.entry("kind").or_insert(json!("manual"));
            map.entry("at").or_insert_with(|| json!(journal::now_text()));
        }
        let record = RunRecord {
            run_id: request.run_id.clone(),
            flow: flow_name,
            scope: request.scope.clone(),
            flow_hash: crate::validate::hash(&request.flow),
            status: RunStatus::Queued,
            trigger: trigger_summary(&trigger),
            inputs: request.inputs.clone(),
            outputs: Value::Null,
            error: None,
            waiting_for: Value::Null,
            created_at: journal::now_text(),
            started_at: None,
            finished_at: None,
            running_ms: 0,
            steps_done: 0,
            task_id: request.task_id.clone(),
            parent: request.parent.clone(),
            rerun_of: request.rerun_of.clone(),
            dry_run: request.dry_run,
            attempt: 0,
            flow_dir: request.flow_dir.as_ref().map(|d| d.display().to_string()),
        };
        let dir = RunDir::create(&self.paths, &record, &request.flow, &trigger).map_err(|e| FlowError::new(Kind::Unavailable, format!("cannot write the run's folder: {e}")))?;
        dir.append("run_created", json!({ "run_id": record.run_id, "trigger": record.trigger })).map_err(|e| FlowError::new(Kind::Unavailable, format!("cannot write the run's journal: {e}")))?;
        Ok(Created { dir, record, request: RunRequest { trigger, ..request }, resume: None, copy: false })
    }

    /// A run that was `queued` or `running` when Automation stopped, ready to go on
    pub fn reopen(&self, run_dir: std::path::PathBuf, permissions: Value) -> Option<Created> {
        let mut record = journal::read_run(&run_dir)?;
        if record.status.is_finished() {
            return None;
        }
        let flow = journal::read_json(&run_dir.join("flow.json"))?;
        let trigger = journal::read_json(&run_dir.join("trigger.json")).unwrap_or_else(|| json!({ "kind": "manual" }));
        let journal = journal::read_journal(&run_dir);
        let dir = RunDir::open(run_dir).ok()?;
        record.attempt += 1;
        let _ = dir.append("run_resumed", json!({ "attempt": record.attempt }));
        let request = RunRequest {
            flow,
            scope: record.scope.clone(),
            permissions,
            inputs: record.inputs.clone(),
            trigger,
            dry_run: record.dry_run,
            mocks: Map::new(),
            run_id: record.run_id.clone(),
            task_id: record.task_id.clone(),
            parent: record.parent.clone(),
            rerun_of: record.rerun_of.clone(),
            flow_dir: record.flow_dir.clone().map(std::path::PathBuf::from),
        };
        Some(Created { dir, record, request, resume: Some(journal), copy: false })
    }

    /// A new run of `request` that reuses the records of `earlier` (another run's folder) for the
    /// steps that finished before `from` first started; `from` and what follows run again
    pub fn rerun(&self, request: RunRequest, earlier: &std::path::Path, from: &str) -> Result<Created, FlowError> {
        let mut ids = Vec::new();
        crate::format::walk_steps(&request.flow, &mut |step| ids.push(step.id.clone()));
        if !ids.iter().any(|id| id == from) {
            return Err(FlowError::new(Kind::NotFound, format!("the flow has no step `{from}`")));
        }
        let old = journal::read_journal(earlier);
        let cut = *old.started.get(from).ok_or_else(|| FlowError::new(Kind::NotFound, format!("the earlier run never reached `{from}`")))?;
        let copied = old.finished[..cut.min(old.finished.len())].iter().filter(|r| r.id != from).cloned().collect();
        let mut created = self.create(request)?;
        created.resume = Some(journal::Journal { finished: copied, ..Default::default() });
        created.copy = true;
        Ok(created)
    }

    /// Runs it to the end and returns its final record
    pub async fn execute(&self, created: Created, cancel: Arc<AtomicBool>, events: Events) -> RunRecord {
        let Created { mut dir, mut record, request, resume, copy } = created;
        let flow = Arc::new(request.flow);
        dir.set_secrets(journal::secret_values(&flow));
        let limits = Limits::of(&flow);
        let zone = Zone::parse(flow.get("timezone").and_then(Value::as_str)).unwrap_or(Zone::Local);
        record.status = RunStatus::Running;
        record.started_at.get_or_insert_with(journal::now_text);
        let hook_token = journal::hook_token(&dir.dir);
        let webhook_url = self.host.webhook_url(&record.run_id, &hook_token);
        let run_value = json!({ "id": record.run_id, "flow": record.flow, "started_at": record.started_at, "attempt": record.attempt, "webhook_url": webhook_url });
        let previous = state::previous(&self.paths, &record.scope, &record.flow);
        let (done, unfinished, waits) = match &resume {
            Some(journal) => (journal.finished_by_key(), journal.unfinished.iter().map(|(p, i)| journal::record_key(p, i)).collect(), journal.waiting.clone()),
            None => (HashMap::new(), HashSet::new(), HashMap::new()),
        };
        if resume.is_some() {
            // Steps that finished before the restart are counted once, whether or not run.json saw them
            record.steps_done = done.len() as u64;
        }
        let info = RunInfo { flow: record.flow.clone(), scope: record.scope.clone(), run_id: record.run_id.clone(), permissions: request.permissions.clone(), flow_dir: request.flow_dir.clone() };
        let ctx = Context::new(Value::Null, request.trigger.clone(), run_value, zone, previous, self.paths.flows.clone());
        let shared = Shared {
            dir,
            record: Mutex::new(record),
            limits,
            dry_run: request.dry_run,
            mocks: request.mocks,
            done: Mutex::new(done),
            unfinished,
            waits,
            steps_run: AtomicU64::new(0),
            events,
            clock: RunClock::new(),
            slots: tokio::sync::Semaphore::new(limits.max_parallel),
            hook_token,
            outside_content: crate::validate::uses::outside_triggers(&flow),
            copy_records: copy,
        };
        let mut st = RunState { info, ctx, run: Arc::new(shared), stops: vec![cancel] };
        st.save();
        let summary = st.run.record.lock().map(|r| r.trigger.clone()).unwrap_or_default();
        st.event("run.started", json!({ "run_id": st.info.run_id, "trigger": summary }));

        let outcome = match prepare_inputs(&flow, &request.inputs) {
            Err(error) => Err(Stop::Fail(error)),
            Ok(inputs) => {
                st.ctx.inputs = inputs.clone();
                st.update(|r| r.inputs = inputs);
                let steps = flow.get("steps").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                let watch = timeout_watch(st.run.clone(), limits.timeout_ms);
                tokio::select! {
                    result = self.run_list(&mut st, steps, None, "steps", "steps", &Value::Null) => result,
                    _ = watch => Err(Stop::Fail(FlowError::new(Kind::Limit, "the run ran longer than limits.timeout (time spent waiting not counted)"))),
                }
            }
        };
        self.finish(&mut st, &flow, outcome).await;
        let record = st.run.record.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // What the run's task returns is masked like its files
        let mut value = serde_json::to_value(&record).unwrap_or_default();
        journal::mask(&mut value, st.run.dir.secrets());
        serde_json::from_value(value).unwrap_or(record)
    }

    async fn finish(&self, st: &mut RunState, flow: &Value, outcome: Result<Value, Stop>) {
        let (status, outputs, error) = match outcome {
            Ok(_) => {
                let outputs = match flow.get("outputs") {
                    Some(outputs) => st.ctx.resolve(outputs).unwrap_or(Value::Null),
                    None => Value::Null,
                };
                (RunStatus::Succeeded, outputs, None)
            }
            Err(Stop::Return(value)) => (RunStatus::Succeeded, value, None),
            Err(Stop::Cancelled) => (RunStatus::Cancelled, Value::Null, Some(FlowError::new(Kind::Cancelled, "the run was cancelled"))),
            Err(Stop::Fail(error)) => {
                let mut error = error;
                if let Some(handlers) = flow.get("on_error").and_then(Value::as_array).filter(|h| !h.is_empty()) {
                    st.ctx.push_local("error", error.to_value());
                    st.ctx.push_scope();
                    let handled = self.run_list(st, handlers, None, "on_error", "on_error", &Value::Null).await;
                    st.ctx.pop_scope();
                    st.ctx.pop_locals(1);
                    if let Err(Stop::Fail(second)) = handled {
                        error.details = json!({ "details": error.details, "on_error_failed": second });
                    }
                }
                (RunStatus::Failed, Value::Null, Some(error))
            }
        };
        if status == RunStatus::Succeeded && !st.run.dry_run {
            state::keep_previous(&self.paths, &st.info.scope, &st.info.flow, &st.ctx.watched);
        }
        let running_ms = st.run.clock.running_ms();
        st.update(|r| {
            r.status = status;
            r.outputs = outputs.clone();
            r.error = error.clone();
            r.waiting_for = Value::Null;
            r.finished_at = Some(journal::now_text());
            r.running_ms += running_ms;
        });
        st.log("run_ended", json!({ "status": status, "error": error }));
        st.event("run.finished", json!({ "status": status, "outputs": outputs, "error": error }));
    }
}

/// Ends when the run has been running (not waiting) longer than `timeout_ms`
async fn timeout_watch(run: Arc<Shared>, timeout_ms: i64) {
    let limit = timeout_ms.max(0) as u64;
    loop {
        let used = run.clock.running_ms();
        if !run.clock.waiting() && used >= limit {
            return;
        }
        let left = limit.saturating_sub(used).clamp(50, 1_000);
        tokio::time::sleep(Duration::from_millis(left)).await;
    }
}

/// The run's inputs: defaults filled in, then checked against `inputs`
pub fn prepare_inputs(flow: &Value, given: &Value) -> Result<Value, FlowError> {
    let declared = flow.get("inputs").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut inputs = match given {
        Value::Null => Map::new(),
        Value::Object(map) => map.clone(),
        other => return Err(FlowError::new(Kind::InvalidInput, format!("inputs must be an object, not {other}"))),
    };
    schema::apply_defaults(&mut inputs, &declared);
    let required: Vec<&String> = declared.iter().filter(|(_, s)| s.get("default").is_none()).map(|(k, _)| k).collect();
    let whole = json!({ "type": "object", "properties": declared, "required": required, "additionalProperties": false });
    let errors = schema::validate(&Value::Object(inputs.clone()), &whole);
    if !errors.is_empty() {
        let list: Vec<Value> = errors.iter().map(|e| e.to_value()).collect();
        let text: Vec<String> = errors.iter().map(|e| format!("inputs{}: {}", e.path.replace('/', "."), e.message)).collect();
        return Err(FlowError::new(Kind::InvalidInput, text.join("; ")).with_details(json!({ "errors": list })));
    }
    Ok(Value::Object(inputs))
}
