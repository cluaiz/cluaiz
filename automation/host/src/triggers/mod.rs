//! Listening for the triggers of enabled flows (docs/automation/triggers.md): clocks, polls,
//! folders, webhooks and Engine events. Every event goes through [`fire`], which checks the flow,
//! its permissions and the trigger's `filter` / `dedupe`, then starts a run as `flows.run` does.

mod clock;
mod events;
mod poller;
mod watcher;

pub use events::listen;

use crate::service::{self, concurrency, Launch, Launched, Service};
use cluaiz_automation::journal::Paths;
use cluaiz_automation::run::prepare_inputs;
use cluaiz_automation::store::Scope;
use cluaiz_automation::trigger::webhook::Hook;
use cluaiz_automation::trigger::{self, state, state::Count, Admit, Owner};
use cluaiz_automation::validate;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Webhook requests one flow takes per minute (*proposed*)
const HOOKS_PER_MINUTE: u32 = 60;

/// What a trigger is doing now, for `flows.get` and `flows.list`
#[derive(Debug, Clone)]
pub struct Live {
    /// `active`, `disabled`, `waiting_for_permission`, `error`, `not_built`, `manual`
    pub status: &'static str,
    pub next_at: Option<String>,
    pub message: Option<String>,
}

impl Live {
    fn new(status: &'static str) -> Self {
        Self { status, next_at: None, message: None }
    }
    fn error(message: impl Into<String>) -> Self {
        Self { status: "error", next_at: None, message: Some(message.into()) }
    }
}

/// The live state of one flow's triggers, shared with the tasks that listen for them
pub type LiveMap = Arc<Mutex<HashMap<String, Live>>>;

pub fn set_live(live: &LiveMap, id: &str, change: impl FnOnce(&mut Live)) {
    let mut map = live.lock().unwrap_or_else(|e| e.into_inner());
    change(map.entry(id.to_string()).or_insert_with(|| Live::new("active")));
}

struct Active {
    tasks: Vec<tokio::task::AbortHandle>,
    live: LiveMap,
    hooks: Vec<(String, Hook)>,
    /// Trigger id → Engine event name
    engine: Vec<(String, String)>,
    /// Trigger id → the name of a flow's `emit` it starts on
    emits: Vec<(String, String)>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.tasks.iter().for_each(tokio::task::AbortHandle::abort);
    }
}

#[derive(Default)]
pub struct Triggers {
    active: Mutex<HashMap<String, Active>>,
    /// Webhook requests this minute, per flow
    rate: Mutex<HashMap<String, (i64, u32)>>,
    /// Flows are loaded and listening (webhooks answer `automation_starting` before)
    pub ready: AtomicBool,
}

fn key(scope: Scope, name: &str) -> String {
    format!("{}/{name}", scope.as_str())
}

/// What became of one event
pub enum Fired {
    Started(Launched),
    Filtered,
    Duplicate,
    /// `concurrency: skip` and a run is going, or the queue is full
    Skipped(String),
    /// The flow is no longer enabled, or has no such trigger
    Gone,
    /// The flow cannot run now (errors, permissions, inputs)
    Failed(String),
}

impl Triggers {
    fn active(&self) -> std::sync::MutexGuard<'_, HashMap<String, Active>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn stop(&self, scope: Scope, name: &str) {
        self.active().remove(&key(scope, name));
    }

    /// Each trigger of a flow with its live state and what it kept (counters, last error)
    pub fn statuses(&self, paths: &Paths, scope: Scope, flow: &Value, enabled: bool) -> Vec<Value> {
        let name = flow.get("name").and_then(Value::as_str).unwrap_or_default();
        let live: HashMap<String, Live> = self.active().get(&key(scope, name)).map(|a| a.live.lock().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or_default();
        let owner = Owner { paths, scope: scope.as_str(), flow };
        let kept = state::read(&owner);
        trigger::specs(flow)
            .iter()
            .map(|spec| {
                let now = live.get(&spec.id).cloned().unwrap_or_else(|| Live::new(if !spec.built() { "not_built" } else if spec.kind == "manual" { "manual" } else if enabled { "error" } else { "disabled" }));
                let kept = kept.get(&spec.id);
                let mut out = json!({ "id": spec.id, "kind": spec.kind, "status": now.status });
                if let Some(next) = now.next_at {
                    out["next_at"] = json!(next);
                }
                if let Some(message) = now.message {
                    out["message"] = json!(message);
                }
                if let Some(error) = kept.and_then(|k| k.get("last_error")).filter(|e| !e.is_null()) {
                    out["last_error"] = error.clone();
                }
                if let Some(counts) = kept.and_then(|k| k.get("counts")) {
                    out["counts"] = counts.clone();
                }
                if spec.kind == "webhook" {
                    out["path"] = json!(format!("/v1/hooks/{name}/{}", spec.id));
                }
                out
            })
            .collect()
    }

    /// The webhook at `/v1/hooks/<name>[/<trigger>]`: the project's flow first. Without a trigger
    /// id, only a flow with one webhook answers.
    pub fn hook(&self, name: &str, trigger: Option<&str>) -> Option<(Scope, String, Hook)> {
        let active = self.active();
        [Scope::Project, Scope::User].into_iter().find_map(|scope| {
            let hooks = &active.get(&key(scope, name))?.hooks;
            let (id, hook) = match trigger {
                Some(id) => hooks.iter().find(|(h, _)| h == id)?,
                None if hooks.len() == 1 => &hooks[0],
                None => return None,
            };
            Some((scope, id.clone(), hook.clone()))
        })
    }

    /// Flows waiting for an Engine event: (scope, flow, trigger id)
    pub fn listening_for(&self, event: &str) -> Vec<(Scope, String, String)> {
        let active = self.active();
        let mut out = Vec::new();
        for (flow_key, flow) in active.iter() {
            let Some((scope, name)) = flow_key.split_once('/').and_then(|(s, n)| Some((Scope::parse(s)?, n.to_string()))) else { continue };
            out.extend(flow.engine.iter().filter(|(_, e)| e == event).map(|(id, _)| (scope, name.clone(), id.clone())));
        }
        out
    }

    /// Flows that start on a flow's `emit` of `name`: (scope, flow, trigger id). A flow never
    /// starts on its own events, so an event cannot start the flow that sends it again and again.
    pub fn listening_for_emit(&self, name: &str, from_scope: Scope, from_flow: &str) -> Vec<(Scope, String, String)> {
        let active = self.active();
        let mut out = Vec::new();
        for (flow_key, flow) in active.iter() {
            let Some((scope, flow_name)) = flow_key.split_once('/').and_then(|(s, n)| Some((Scope::parse(s)?, n.to_string()))) else { continue };
            if scope == from_scope && flow_name == from_flow {
                continue;
            }
            out.extend(flow.emits.iter().filter(|(_, e)| e == name).map(|(id, _)| (scope, flow_name.clone(), id.clone())));
        }
        out
    }

    /// One more webhook request for this flow this minute; `false` past the limit
    pub fn take_hook_turn(&self, name: &str) -> bool {
        let minute = chrono::Utc::now().timestamp() / 60;
        let mut rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
        let entry = rate.entry(name.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        entry.1 += 1;
        entry.1 <= HOOKS_PER_MINUTE
    }
}

/// Starts listening for every flow's triggers (at start: `startup` triggers run once)
pub async fn sync_all(service: &Arc<Service>, startup: bool) {
    let store = service.store().await;
    for file in store.list(None) {
        sync(service, file.scope, &file.name, startup).await;
    }
    service.triggers.ready.store(true, Ordering::Relaxed);
}

/// Listens for one flow's triggers as its file and settings say now: after a save, an enable or
/// a disable. Old listeners stop first.
pub async fn sync(service: &Arc<Service>, scope: Scope, name: &str, startup: bool) {
    service.triggers.stop(scope, name);
    let store = service.store().await;
    let Some(file) = store.find(name, Some(scope)) else { return };
    let live: LiveMap = Arc::new(Mutex::new(HashMap::new()));
    let mut active = Active { tasks: Vec::new(), live: live.clone(), hooks: Vec::new(), engine: Vec::new(), emits: Vec::new() };
    let Ok(flow) = file.flow else {
        service.triggers.active().insert(key(scope, name), active);
        return;
    };
    let specs = trigger::specs(&flow);
    let settings = store.settings(scope, name);
    let set_all = |status: Live| {
        let mut map = live.lock().unwrap_or_else(|e| e.into_inner());
        for spec in &specs {
            let now = if !spec.built() { Live::new("not_built") } else if spec.kind == "manual" { Live::new("manual") } else { status.clone() };
            map.insert(spec.id.clone(), now);
        }
    };
    let report = validate::validate(&flow, Some(name), &service.snapshot().await);
    if settings["enabled"].as_bool() != Some(true) {
        set_all(Live::new("disabled"));
    } else if !report.valid {
        set_all(Live::error("the flow has errors; flows.validate lists them"));
    } else if service::unconfirmed(&flow, &report.permissions_needed, &settings["permissions"]).is_some() {
        set_all(Live { status: "waiting_for_permission", next_at: None, message: Some("the file asks for permissions not confirmed; flows.enable with permissions_confirmed".into()) });
    } else {
        let switched_off: Vec<&str> = settings["disabled_triggers"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let permissions = settings["permissions"].clone();
        for spec in specs.iter() {
            let id = spec.id.clone();
            if !spec.built() {
                set_live(&live, &id, |l| *l = Live::new("not_built"));
                continue;
            }
            if !spec.enabled() || switched_off.contains(&id.as_str()) {
                set_live(&live, &id, |l| *l = Live::new("disabled"));
                continue;
            }
            let at = Place { service: service.clone(), scope, name: name.to_string(), live: live.clone(), id: id.clone() };
            set_live(&live, &id, |l| *l = Live::new("active"));
            match spec.kind {
                "manual" => set_live(&live, &id, |l| *l = Live::new("manual")),
                "schedule" | "every" => active.tasks.push(tokio::spawn(clock::run(at, spec.clone(), flow.clone())).abort_handle()),
                "poll" => active.tasks.push(tokio::spawn(poller::run(at, spec.clone(), flow.clone(), permissions.clone())).abort_handle()),
                "file" => active.tasks.push(tokio::spawn(watcher::run(at, spec.clone())).abort_handle()),
                "webhook" => match Hook::of(spec) {
                    Ok(hook) => active.hooks.push((id, hook)),
                    Err(message) => set_live(&live, &id, |l| *l = Live::error(message)),
                },
                "engine" => active.engine.push((id, spec.body().as_str().unwrap_or_default().to_string())),
                "event" => active.emits.push((id, spec.body().as_str().unwrap_or_default().to_string())),
                "startup" if startup => {
                    active.tasks.push(tokio::spawn(async move { at.fire(json!({})).await }).abort_handle());
                }
                _ => {}
            }
        }
    }
    service.triggers.active().insert(key(scope, name), active);
}

/// Where a listening task reports: its flow and trigger
#[derive(Clone)]
pub struct Place {
    pub service: Arc<Service>,
    pub scope: Scope,
    pub name: String,
    pub live: LiveMap,
    pub id: String,
}

impl Place {
    pub fn set(&self, change: impl FnOnce(&mut Live)) {
        set_live(&self.live, &self.id, change);
    }

    pub async fn fire(&self, data: Value) -> Fired {
        let fired = fire(&self.service, self.scope, &self.name, &self.id, data).await;
        if let Fired::Failed(message) = &fired {
            tracing::warn!("Automation: trigger {} of {} did not start a run: {message}", self.id, self.name);
        }
        fired
    }
}

/// One event of a trigger: the flow is read again, checked, and a run starts if every check passes
pub async fn fire(service: &Arc<Service>, scope: Scope, name: &str, id: &str, data: Value) -> Fired {
    let store = service.store().await;
    let Some(file) = store.find(name, Some(scope)) else { return Fired::Gone };
    let Ok(flow) = file.flow else { return Fired::Gone };
    let settings = store.settings(scope, name);
    let switched_off = settings["disabled_triggers"].as_array().is_some_and(|a| a.iter().any(|t| t == id));
    let Some(spec) = trigger::specs(&flow).into_iter().find(|s| s.id == id) else { return Fired::Gone };
    if settings["enabled"].as_bool() != Some(true) || switched_off || !spec.enabled() {
        return Fired::Gone;
    }
    let owner = Owner { paths: &service.paths, scope: scope.as_str(), flow: &flow };
    let failed = |message: String| {
        state::failed(&owner, id, &message);
        Fired::Failed(message)
    };
    let catalog = service.snapshot().await;
    let report = validate::validate(&flow, Some(name), &catalog);
    if !report.valid {
        return failed("the flow has errors; flows.validate lists them".into());
    }
    if service::unconfirmed(&flow, &report.permissions_needed, &settings["permissions"]).is_some() {
        return failed("the file asks for permissions the owner has not confirmed".into());
    }
    let (trigger_data, inputs) = match trigger::admit(&owner, &spec, data, chrono::Utc::now()) {
        Ok(Admit::Run { trigger, inputs }) => (trigger, inputs),
        Ok(Admit::Filtered) => {
            state::count(&owner, id, Count::Filtered);
            return Fired::Filtered;
        }
        Ok(Admit::Duplicate) => {
            state::count(&owner, id, Count::Duplicate);
            return Fired::Duplicate;
        }
        Err(error) => return failed(format!("{}: {}", error.kind, error.message)),
    };
    if let Err(error) = prepare_inputs(&flow, &inputs) {
        return failed(format!("the trigger's inputs: {}", error.message));
    }
    let mode = concurrency(&flow);
    let flow_dir = file.path.parent().map(std::path::PathBuf::from);
    let launch = Launch { flow: flow.clone(), scope, permissions: settings["permissions"].clone(), inputs, trigger: trigger_data, dry_run: false, mocks: Default::default(), mode, principal: "owner".into(), parent: Value::Null, flow_dir };
    match service.launch(service.runner(catalog), launch) {
        Ok(launched) => {
            state::count(&owner, id, Count::Runs);
            state::worked(&owner, id);
            tracing::info!("Automation: {} started run {} of {name}", spec.id, launched.run_id);
            Fired::Started(launched)
        }
        Err(error) if error.data.as_ref().and_then(|d| d.get("type")).and_then(Value::as_str) == Some("skipped") => {
            state::count(&owner, id, Count::Skipped);
            Fired::Skipped(error.message)
        }
        Err(error) => failed(error.message),
    }
}
