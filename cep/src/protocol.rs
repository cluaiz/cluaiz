//! The protocol's own methods: `rpc.discover` and `tasks.*` (01-architecture §7.6, §9)

use crate::error::CepError;
use crate::principal::Scope;
use crate::registry::{Call, Meta, Registry};
use crate::task::MAX_WAIT_MS;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Discover {
    /// Only this group, for example "fs"
    #[serde(default)]
    group: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskGet {
    task_id: String,
    /// Events after this cursor (`next` of the last answer); 0 or absent = all kept events
    #[serde(default)]
    after: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskResult {
    task_id: String,
    /// How long to wait for the end, at most 30000 ms; 0 or absent = do not wait
    #[serde(default)]
    wait_ms: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskId {
    task_id: String,
}

pub(crate) fn register<Ctx: Send + 'static>(registry: &mut Registry<Ctx>) {
    let task = json!({ "task_id": "t_5f0c…", "method": "ingest.run", "status": "working", "created_at": "2026-10-25T10:30:00Z",
                       "last_updated_at": "2026-10-25T10:30:02Z", "ttl_ms": 3600000, "poll_ms": 500 });
    registry
        .add(
            "rpc.discover",
            Meta::new(Scope::Read, "The OpenRPC description of the methods you may call").no_mcp().example(
                json!({ "group": "tasks" }),
                json!({ "openrpc": "1.3.2", "info": { "title": "Cluaiz Engine Protocol" }, "methods": [] }),
            ),
            |_: Ctx, call: Call, p: Discover| async move {
                let discovery = call.shared.discovery.get().ok_or_else(|| CepError::unavailable("discovery_not_ready", "Starting"))?;
                Ok(discovery.document(&call.principal, p.group.as_deref()))
            },
        )
        .add(
            "tasks.get",
            Meta::new(Scope::Read, "A task's status, progress and new events").example(
                json!({ "task_id": "t_5f0c…", "after": 0 }),
                json!({ "status": "working", "progress": { "done": 1, "total": 3 }, "events": [], "next": 0 }),
            ),
            |_: Ctx, call: Call, p: TaskGet| async move { call.shared.tasks.get(&call.principal.id, &p.task_id, p.after) },
        )
        .add(
            "tasks.result",
            Meta::new(Scope::Read, "A task's final result or error; waits up to wait_ms").example(
                json!({ "task_id": "t_5f0c…", "wait_ms": 30000 }),
                json!({ "any": "the result the method returns" }),
            ),
            |_: Ctx, call: Call, p: TaskResult| async move {
                let result: Result<Value, CepError> =
                    call.shared.tasks.result(&call.principal.id, &p.task_id, p.wait_ms.unwrap_or(0).min(MAX_WAIT_MS)).await;
                result
            },
        )
        .add(
            "tasks.cancel",
            Meta::new(Scope::Read, "Cancels a running task").idempotent(false).example(
                json!({ "task_id": "t_5f0c…" }),
                json!({ "status": "cancelled", "status_message": "Cancelled by request" }),
            ),
            |_: Ctx, call: Call, p: TaskId| async move { call.shared.tasks.cancel(&call.principal.id, &p.task_id) },
        )
        .add(
            "tasks.list",
            Meta::new(Scope::Read, "Your tasks that have not expired, newest first")
                .example(json!({}), json!({ "tasks": [task] })),
            |_: Ctx, call: Call, _: crate::registry::NoParams| async move {
                Ok(json!({ "tasks": call.shared.tasks.list(&call.principal.id) }))
            },
        );
}
