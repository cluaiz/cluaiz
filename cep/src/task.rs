//! Tasks for long work (01-architecture §9), with the same states as MCP tasks. A task belongs to
//! the principal that started it; others get `not_found`. Kept in memory only.

use crate::approval::{now_ms, rfc3339};
use crate::error::CepError;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

/// At most this many events are kept per task; older ones are dropped (the cursor still counts them)
const MAX_EVENTS: usize = 1000;
/// Suggested wait between `tasks.get` calls
const POLL_MS: u64 = 500;
/// The longest `tasks.result` waits in one call
pub const MAX_WAIT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Completed | Status::Failed | Status::Cancelled)
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Progress {
    pub done: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

struct Task {
    principal: String,
    method: String,
    status: Status,
    status_message: Option<String>,
    created_ms: u64,
    updated_ms: u64,
    ttl_ms: u64,
    progress: Option<Progress>,
    /// (seq, event), seq counting from 1
    events: Vec<(u64, Value)>,
    next_seq: u64,
    outcome: Option<Result<Value, CepError>>,
    cancel: Arc<AtomicBool>,
    finished: watch::Sender<bool>,
}

impl Task {
    fn view(&self, id: &str, after: Option<u64>) -> Value {
        let mut view = json!({
            "task_id": id,
            "method": self.method,
            "status": self.status,
            "created_at": rfc3339(self.created_ms),
            "last_updated_at": rfc3339(self.updated_ms),
            "ttl_ms": self.ttl_ms,
            "poll_ms": POLL_MS,
        });
        if let Some(message) = &self.status_message {
            view["status_message"] = json!(message);
        }
        if let Some(progress) = &self.progress {
            view["progress"] = json!(progress);
        }
        if let Some(after) = after {
            let events: Vec<&Value> = self.events.iter().filter(|(seq, _)| *seq > after).map(|(_, e)| e).collect();
            view["events"] = json!(events);
            view["next"] = json!(self.next_seq - 1);
        }
        view
    }
}

pub struct TaskStore {
    max_running_per_principal: usize,
    default_ttl_ms: u64,
    max_ttl_ms: u64,
    tasks: Mutex<HashMap<String, Task>>,
}

/// Given to the work of one task: it reports progress and events, and sees a cancel
#[derive(Clone)]
pub struct TaskHandle {
    store: Arc<TaskStore>,
    id: String,
    cancel: Arc<AtomicBool>,
}

impl TaskHandle {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The cancel flag itself, for work that takes an abort flag (a download)
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn progress(&self, progress: Progress) {
        self.store.update(&self.id, |task| task.progress = Some(progress));
    }

    /// A piece of output (a download step, one ingested source, an audio chunk)
    pub fn event(&self, event: Value) {
        self.store.update(&self.id, |task| {
            task.events.push((task.next_seq, event));
            task.next_seq += 1;
            if task.events.len() > MAX_EVENTS {
                task.events.remove(0);
            }
        });
    }
}

impl TaskStore {
    pub fn new(max_running_per_principal: usize, default_ttl_ms: u64, max_ttl_ms: u64) -> Self {
        Self { max_running_per_principal, default_ttl_ms, max_ttl_ms, tasks: Mutex::new(HashMap::new()) }
    }

    fn tasks(&self) -> std::sync::MutexGuard<'_, HashMap<String, Task>> {
        self.tasks.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Task)) {
        if let Some(task) = self.tasks().get_mut(id) {
            change(task);
            task.updated_ms = now_ms();
        }
    }

    /// Starts `work` in the background and returns the task's view. `work` gets a handle for
    /// progress, events and cancel; its result becomes what `tasks.result` returns.
    pub fn start<F, Fut>(self: &Arc<Self>, principal: &str, method: &str, ttl_ms: Option<u64>, work: F) -> Result<Value, CepError>
    where
        F: FnOnce(TaskHandle) -> Fut,
        Fut: Future<Output = Result<Value, CepError>> + Send + 'static,
    {
        let now = now_ms();
        let id = format!("t_{}", uuid::Uuid::new_v4().simple());
        let cancel = Arc::new(AtomicBool::new(false));
        let view = {
            let mut tasks = self.tasks();
            tasks.retain(|_, t| t.created_ms + t.ttl_ms > now);
            let running = tasks.values().filter(|t| t.principal == principal && !t.status.is_terminal()).count();
            if running >= self.max_running_per_principal {
                return Err(CepError::conflict("too_many_tasks", format!("{running} tasks are already running")));
            }
            let task = Task {
                principal: principal.into(),
                method: method.into(),
                status: Status::Working,
                status_message: None,
                created_ms: now,
                updated_ms: now,
                ttl_ms: ttl_ms.unwrap_or(self.default_ttl_ms).min(self.max_ttl_ms),
                progress: None,
                events: Vec::new(),
                next_seq: 1,
                outcome: None,
                cancel: cancel.clone(),
                finished: watch::channel(false).0,
            };
            let view = task.view(&id, None);
            tasks.insert(id.clone(), task);
            view
        };
        let handle = TaskHandle { store: self.clone(), id: id.clone(), cancel };
        let future = work(handle);
        let store = self.clone();
        tokio::spawn(async move {
            let outcome = future.await;
            let mut tasks = store.tasks();
            if let Some(task) = tasks.get_mut(&id) {
                // A cancelled task stays cancelled, whatever the work returned
                if task.status != Status::Cancelled {
                    task.status = if outcome.is_ok() { Status::Completed } else { Status::Failed };
                    if let Err(error) = &outcome {
                        task.status_message = Some(error.message.clone());
                    }
                    task.outcome = Some(outcome);
                }
                task.updated_ms = now_ms();
                let _ = task.finished.send(true);
            }
        });
        Ok(view)
    }

    fn owned<'a>(tasks: &'a mut HashMap<String, Task>, principal: &str, id: &str) -> Result<&'a mut Task, CepError> {
        tasks
            .get_mut(id)
            .filter(|t| t.principal == principal && t.created_ms + t.ttl_ms > now_ms())
            .ok_or_else(|| CepError::not_found("task_not_found", format!("No task {id}")))
    }

    /// Status and progress; with `after`, the events after that cursor too
    pub fn get(&self, principal: &str, id: &str, after: Option<u64>) -> Result<Value, CepError> {
        let mut tasks = self.tasks();
        Ok(Self::owned(&mut tasks, principal, id)?.view(id, Some(after.unwrap_or(0))))
    }

    /// The final result or error, exactly what a direct call would return. Waits up to `wait_ms`;
    /// still running after that is a `timeout` and the caller asks again.
    pub async fn result(&self, principal: &str, id: &str, wait_ms: u64) -> Result<Value, CepError> {
        let mut finished = {
            let mut tasks = self.tasks();
            let task = Self::owned(&mut tasks, principal, id)?;
            if task.status == Status::Cancelled {
                return Err(CepError::cancelled());
            }
            if let Some(outcome) = &task.outcome {
                return outcome.clone();
            }
            task.finished.subscribe()
        };
        let wait = Duration::from_millis(wait_ms.min(MAX_WAIT_MS));
        let _ = tokio::time::timeout(wait, finished.wait_for(|done| *done)).await;
        let mut tasks = self.tasks();
        let task = Self::owned(&mut tasks, principal, id)?;
        match (&task.outcome, task.status) {
            (_, Status::Cancelled) => Err(CepError::cancelled()),
            (Some(outcome), _) => outcome.clone(),
            (None, _) => Err(CepError::timeout(format!("Task {id} is still running"))),
        }
    }

    pub fn cancel(&self, principal: &str, id: &str) -> Result<Value, CepError> {
        let mut tasks = self.tasks();
        let task = Self::owned(&mut tasks, principal, id)?;
        if task.status.is_terminal() {
            return Err(CepError::invalid_params(format!("Task {id} has already ended"), Vec::new(), None));
        }
        task.cancel.store(true, Ordering::Relaxed);
        task.status = Status::Cancelled;
        task.status_message = Some("Cancelled by request".into());
        task.updated_ms = now_ms();
        let _ = task.finished.send(true);
        Ok(task.view(id, None))
    }

    /// The principal's own tasks, newest first
    pub fn list(&self, principal: &str) -> Vec<Value> {
        let now = now_ms();
        let tasks = self.tasks();
        let mut own: Vec<(&String, &Task)> =
            tasks.iter().filter(|(_, t)| t.principal == principal && t.created_ms + t.ttl_ms > now).collect();
        own.sort_by_key(|(_, t)| std::cmp::Reverse(t.created_ms));
        own.into_iter().map(|(id, t)| t.view(id, None)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{CONFLICT, INVALID_PARAMS, NOT_FOUND, TIMEOUT};

    fn store() -> Arc<TaskStore> {
        Arc::new(TaskStore::new(2, 60_000, 120_000))
    }

    #[tokio::test]
    async fn test_task_lifecycle_and_owner() {
        let store = store();
        let view = store
            .start("p", "test.slow", None, |handle| async move {
                handle.progress(Progress { done: 1, total: Some(2), ..Default::default() });
                handle.event(json!({"step": 1}));
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(json!({"answer": 42}))
            })
            .unwrap();
        let id = view["task_id"].as_str().unwrap().to_string();
        assert_eq!(view["status"], "working");
        // Another principal cannot see it
        assert_eq!(store.get("q", &id, None).unwrap_err().code, NOT_FOUND);
        assert_eq!(store.result("p", &id, 0).await.unwrap_err().code, TIMEOUT);
        assert_eq!(store.result("p", &id, 5_000).await.unwrap(), json!({"answer": 42}));
        let done = store.get("p", &id, Some(0)).unwrap();
        assert_eq!((done["status"].as_str(), done["events"].as_array().map(Vec::len)), (Some("completed"), Some(1)));
        assert_eq!(store.get("p", &id, Some(1)).unwrap()["events"], json!([]));
        assert_eq!(store.cancel("p", &id).unwrap_err().code, INVALID_PARAMS);
        assert_eq!(store.list("p").len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_failure_and_limit() {
        let store = store();
        let slow = |_: TaskHandle| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(json!(null))
        };
        let a = store.start("p", "m", None, slow).unwrap()["task_id"].as_str().unwrap().to_string();
        store.start("p", "m", None, slow).unwrap();
        assert_eq!(store.start("p", "m", None, slow).unwrap_err().code, CONFLICT);
        store.cancel("p", &a).unwrap();
        assert_eq!(store.result("p", &a, 1_000).await.unwrap_err().code, crate::error::CANCELLED);
        // A failed task returns the same error a direct call would
        let failed = store
            .start("p", "m", None, |_| async { Err(CepError::not_found("file_not_found", "gone")) })
            .unwrap()["task_id"]
            .as_str()
            .unwrap()
            .to_string();
        let error = store.result("p", &failed, 5_000).await.unwrap_err();
        assert_eq!(error.kind(), Some("file_not_found"));
        assert_eq!(store.get("p", &failed, None).unwrap()["status"], "failed");
    }
}
