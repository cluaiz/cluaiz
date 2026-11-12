//! Automation while the server runs: flow files, runs as CEP tasks, `concurrency`, and runs that
//! go on after a restart.

use crate::catalog::Snapshot;
use crate::engine::EngineDoor;
use cluaiz_api::AppState;
use cluaiz_automation::journal::{self, Paths, RunStatus};
use cluaiz_automation::permissions;
use cluaiz_automation::run::{Created, RunRequest, Runner};
use cluaiz_automation::store::{Scope, Store};
use cluaiz_cep::{CepError, Progress};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Semaphore;

/// A run's task is kept this long after it starts (the run record stays on disk regardless)
const RUN_TASK_TTL_MS: u64 = 24 * 60 * 60 * 1000;
/// Runs of one flow waiting for `concurrency: queue`, at most (*proposed*)
const MAX_QUEUED: usize = 100;
/// Runs are kept per flow at most this many, and this many days (*proposed*)
const KEEP_RUNS: usize = 200;
const KEEP_DAYS: i64 = 30;

struct FlowRuns {
    gate: Arc<Semaphore>,
    running: Vec<(String, Arc<AtomicBool>)>,
    queued: usize,
}

pub struct Service {
    pub paths: Paths,
    pub state: Arc<AppState>,
    flows: Mutex<HashMap<String, FlowRuns>>,
    /// What listens for the triggers of enabled flows
    pub triggers: crate::triggers::Triggers,
}

static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();

/// The one Automation service of this server
pub fn get(state: &Arc<AppState>) -> Arc<Service> {
    SERVICE
        .get_or_init(|| Arc::new(Service { paths: Paths { flows: cluaiz_core::EnvironmentManager::current().flows_dir() }, state: state.clone(), flows: Mutex::new(HashMap::new()), triggers: Default::default() }))
        .clone()
}

/// A run to create and start
pub struct Launch {
    /// The flow, already validated
    pub flow: Value,
    pub scope: Scope,
    /// What the owner confirmed
    pub permissions: Value,
    pub inputs: Value,
    pub trigger: Value,
    pub dry_run: bool,
    pub mocks: Map<String, Value>,
    /// `concurrency`: queue, skip, replace or parallel
    pub mode: String,
    /// Who owns the run's task (sees it with `tasks.get`)
    pub principal: String,
    /// `{ flow, scope, run_id, depth }` of the run whose `run` step started this one
    pub parent: Value,
    /// The folder of the flow file (`code` modules are found there)
    pub flow_dir: Option<std::path::PathBuf>,
}

pub struct Launched {
    pub task: Value,
    pub run_id: String,
    /// `running`, or `queued` behind another run of the flow
    pub status: &'static str,
}

/// The permissions a saved flow asks for, when the owner has not confirmed them all
pub fn unconfirmed(flow: &Value, needed: &Value, confirmed: &Value) -> Option<Value> {
    let wanted = permissions::merge(flow.get("permissions").unwrap_or(&Value::Null), needed);
    (!permissions::covers(confirmed, &wanted)).then_some(wanted)
}

/// `concurrency` of a flow: the short form or the object's `mode`
pub fn concurrency(flow: &Value) -> String {
    match flow.get("concurrency") {
        Some(Value::String(mode)) => mode.clone(),
        Some(Value::Object(rule)) => rule.get("mode").and_then(Value::as_str).unwrap_or("queue").to_string(),
        _ => "queue".into(),
    }
}

impl Service {
    fn flows(&self) -> std::sync::MutexGuard<'_, HashMap<String, FlowRuns>> {
        self.flows.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The flow files, with the current project's when there is one (the Cluaiz folder itself is
    /// not a project)
    pub async fn store(&self) -> Store {
        let workspace = self.state.current_workspace_dir.read().await.clone();
        let home = cluaiz_core::EnvironmentManager::current().local_dir;
        let project = (workspace != home).then_some(workspace);
        Store::new(self.paths.clone(), project.as_deref())
    }

    pub async fn snapshot(&self) -> Snapshot {
        Snapshot::read(&self.state).await
    }

    pub fn runner(&self, catalog: Snapshot) -> Runner {
        Runner::new(Arc::new(EngineDoor { state: self.state.clone(), catalog }), self.paths.clone())
    }

    /// Runs going for a flow, by run id
    pub fn running(&self, scope: &str, name: &str) -> Vec<String> {
        self.flows().get(&format!("{scope}/{name}")).map(|f| f.running.iter().map(|(id, _)| id.clone()).collect()).unwrap_or_default()
    }

    pub fn cancel_flow(&self, scope: &str, name: &str) -> usize {
        let flows = self.flows();
        let Some(runs) = flows.get(&format!("{scope}/{name}")) else { return 0 };
        runs.running.iter().for_each(|(_, cancel)| cancel.store(true, Ordering::Relaxed));
        runs.running.len()
    }

    /// Starts a created run as a CEP task of `principal`, following `concurrency`. Answers at once
    /// with the task's view.
    pub fn start(self: &Arc<Self>, runner: Runner, mut created: Created, principal: &str, mode: &str) -> Result<Value, CepError> {
        let record = created.record().clone();
        let key = format!("{}/{}", record.scope, record.flow);
        let gate = {
            let mut flows = self.flows();
            let runs = flows.entry(key.clone()).or_insert_with(|| FlowRuns { gate: Arc::new(Semaphore::new(1)), running: Vec::new(), queued: 0 });
            let busy = !runs.running.is_empty() || runs.queued > 0;
            match mode {
                "skip" if busy => return Err(CepError::conflict("skipped", format!("a run of {} is going (concurrency: skip)", record.flow))),
                "queue" if runs.queued >= MAX_QUEUED => return Err(CepError::conflict("skipped", format!("{MAX_QUEUED} runs of {} are already queued", record.flow))),
                "replace" => runs.running.iter().for_each(|(_, cancel)| cancel.store(true, Ordering::Relaxed)),
                _ => {}
            }
            runs.queued += 1;
            runs.gate.clone()
        };
        let service = self.clone();
        let mode = mode.to_string();
        let (send, receive) = tokio::sync::oneshot::channel::<Created>();
        let view = self.state.cep.shared().tasks.start(principal, "flows.run", Some(RUN_TASK_TTL_MS), move |handle| async move {
            let created = receive.await.map_err(|_| CepError::internal("flows.run"))?;
            let _permit = if mode == "parallel" { None } else { gate.acquire_owned().await.ok() };
            let cancel = handle.cancel_flag();
            let run_id = created.run_id().to_string();
            {
                let mut flows = service.flows();
                if let Some(runs) = flows.get_mut(&key) {
                    runs.queued = runs.queued.saturating_sub(1);
                    runs.running.push((run_id.clone(), cancel.clone()));
                }
            }
            let reporter = handle.clone();
            let steps_done = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let events = Arc::new(move |event: Value| {
                if event["event"] == "step.finished" {
                    let done = steps_done.fetch_add(1, Ordering::Relaxed) + 1;
                    reporter.progress(Progress { done, total: None, unit: Some("steps".into()), message: event["data"]["id"].as_str().map(String::from) });
                }
                reporter.event(event);
            });
            let record = runner.execute(created, cancel, events).await;
            {
                let mut flows = service.flows();
                if let Some(runs) = flows.get_mut(&key) {
                    runs.running.retain(|(id, _)| id != &run_id);
                }
            }
            journal::prune(&service.paths, &record.flow, KEEP_RUNS, KEEP_DAYS);
            match record.status {
                RunStatus::Succeeded => cluaiz_cep::events::publish("flow.finished", json!({ "flow": record.flow, "scope": record.scope, "run_id": record.run_id, "outputs": record.outputs })),
                RunStatus::Failed => cluaiz_cep::events::publish("flow.failed", json!({ "flow": record.flow, "scope": record.scope, "run_id": record.run_id, "error": record.error })),
                _ => {}
            }
            serde_json::to_value(&record).map_err(|_| CepError::internal("flows.run"))
        });
        let view = match view {
            Ok(view) => view,
            Err(error) => {
                if let Some(runs) = self.flows().get_mut(&format!("{}/{}", record.scope, record.flow)) {
                    runs.queued = runs.queued.saturating_sub(1);
                }
                return Err(error);
            }
        };
        if let Some(task_id) = view.get("task_id").and_then(Value::as_str) {
            created.set_task_id(task_id);
        }
        let _ = send.send(created);
        Ok(view)
    }

    /// Creates a run and starts it: `flows.run` and every trigger come here
    pub fn launch(self: &Arc<Self>, runner: Runner, launch: Launch) -> Result<Launched, CepError> {
        let Launch { flow, scope, permissions, inputs, trigger, dry_run, mocks, mode, principal, parent, flow_dir } = launch;
        let request = RunRequest {
            flow,
            scope: scope.as_str().to_string(),
            permissions,
            inputs,
            trigger,
            dry_run,
            mocks,
            run_id: journal::new_run_id(),
            task_id: None,
            parent,
            rerun_of: Value::Null,
            flow_dir,
        };
        let created = runner.create(request).map_err(|e| CepError::unavailable("automation_storage", e.message))?;
        let run_id = created.run_id().to_string();
        let queued = mode != "parallel" && !self.running(scope.as_str(), &created.record().flow).is_empty();
        let task = self.start(runner, created, &principal, &mode)?;
        Ok(Launched { task, run_id, status: if queued { "queued" } else { "running" } })
    }

    /// At start: runs that were queued, running or waiting when Automation stopped go on
    pub fn resume_all(self: &Arc<Self>) {
        let service = self.clone();
        tokio::spawn(async move {
            let unfinished: Vec<_> = journal::list_runs(&service.paths, None).into_iter().filter(|r| !r.status.is_finished()).collect();
            if unfinished.is_empty() {
                return;
            }
            let store = service.store().await;
            for run in unfinished {
                let Some(scope) = Scope::parse(&run.scope) else { continue };
                let permissions = store.settings(scope, &run.flow)["permissions"].clone();
                let catalog = service.snapshot().await;
                let runner = service.runner(catalog);
                let Some(created) = runner.reopen(service.paths.run_dir(&run.flow, &run.run_id), permissions) else { continue };
                let flow = journal::read_json(&service.paths.run_dir(&run.flow, &run.run_id).join("flow.json")).unwrap_or(json!({}));
                match service.start(runner, created, "owner", &concurrency(&flow)) {
                    Ok(_) => tracing::info!("Automation: run {} of {} goes on after a restart", run.run_id, run.flow),
                    Err(error) => tracing::warn!("Automation: run {} of {} could not go on: {}", run.run_id, run.flow, error.message),
                }
            }
        });
    }
}
