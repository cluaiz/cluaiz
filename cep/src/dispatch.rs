//! One entry for every transport: bytes in, bytes out (01-architecture §6, §12). HTTP, the native
//! pipe, the C ABI and the MCP facade all call `Engine::call` or `Engine::invoke`.

use crate::approval::ApprovalStore;
use crate::discover::Discovery;
use crate::envelope::{self, Incoming, Item, Meta as CallMeta};
use crate::error::{self, CepError};
use crate::principal::{Principal, Scope};
use crate::registry::{digest_of, Call, Registry, Shared};
use crate::task::TaskStore;
use serde_json::{Map, Value};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_batch: usize,
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_batch: 32, max_depth: 64 }
    }
}

/// One line of the audit log: written for write, exec and admin calls and for every denial
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    pub principal: &'a str,
    pub method: &'a str,
    pub scope: Option<Scope>,
    pub digest: [u8; 32],
    /// The error code, or `None` for success
    pub code: Option<i32>,
    pub ms: u64,
}

pub trait Audit: Send + Sync {
    fn record(&self, entry: &AuditEntry);
}

pub struct Engine<Ctx> {
    registry: Registry<Ctx>,
    shared: Arc<Shared>,
    limits: Limits,
    audit: Option<Arc<dyn Audit>>,
}

impl<Ctx: Clone + Send + 'static> Engine<Ctx> {
    pub fn new(registry: Registry<Ctx>, approvals: ApprovalStore, tasks: TaskStore, limits: Limits, title: &str, version: &str) -> Self {
        let shared = Arc::new(Shared { approvals, tasks: Arc::new(tasks), discovery: OnceLock::new() });
        let _ = shared.discovery.set(Discovery::build(&registry, title, version));
        Self { registry, shared, limits, audit: None }
    }

    pub fn with_audit(mut self, audit: Arc<dyn Audit>) -> Self {
        self.audit = Some(audit);
        self
    }

    pub fn registry(&self) -> &Registry<Ctx> {
        &self.registry
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// A body in, the response body out; `None` when there is nothing to answer (notifications)
    pub async fn call(&self, ctx: Ctx, principal: Arc<Principal>, body: &[u8]) -> Option<Vec<u8>> {
        let reply = match envelope::parse(body, self.limits.max_batch, self.limits.max_depth) {
            Err(error) => Some(envelope::err(Value::Null, &error)),
            Ok(Incoming::Single(item)) => self.item(ctx, &principal, item).await,
            Ok(Incoming::Batch(items)) => {
                // In order, one after another: predictable for scripts and small models
                let mut replies = Vec::new();
                for item in items {
                    if let Some(reply) = self.item(ctx.clone(), &principal, item).await {
                        replies.push(reply);
                    }
                }
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
        };
        reply.map(|reply| serde_json::to_vec(&reply).unwrap_or_default())
    }

    async fn item(&self, ctx: Ctx, principal: &Arc<Principal>, item: Item) -> Option<Value> {
        match item {
            Err((id, error)) => Some(envelope::err(id, &error)),
            Ok(request) => {
                let outcome = self.invoke(ctx, principal.clone(), &request.method, request.params, request.meta).await;
                let id = request.id?;
                Some(match outcome {
                    Ok(result) => envelope::ok(id, result),
                    Err(error) => envelope::err(id, &error),
                })
            }
        }
    }

    /// One call by name: lookup, scope check, params check, the method, audit
    pub async fn invoke(&self, ctx: Ctx, principal: Arc<Principal>, method: &str, params: Map<String, Value>, meta: CallMeta) -> Result<Value, CepError> {
        let started = Instant::now();
        let digest = digest_of(&params);
        let Some(entry) = self.registry.get(method) else {
            let (close, group) = self.registry.suggestions(method);
            return Err(CepError::method_not_found(method, close, group));
        };
        if let Err(denied) = principal.allow(entry.meta.scope, entry.meta.owner_only) {
            self.record(&principal, entry.name, Some(entry.meta.scope), digest, Some(denied.code), started);
            return Err(denied);
        }
        let call = Call::new(principal.clone(), entry.name, digest, meta.approval, self.shared.clone());
        let future = (entry.handler)(ctx, call, Value::Object(params));
        // A panic in a method becomes an internal error, never a dropped connection
        let outcome = match tokio::spawn(future).await {
            Ok(outcome) => outcome,
            Err(join) => {
                let trace = uuid::Uuid::new_v4().simple().to_string();
                tracing::error!("CEP method {method} failed (trace {trace}): {join}");
                Err(CepError::internal(&trace))
            }
        };
        let code = outcome.as_ref().err().map(|e| e.code);
        let denied = matches!(code, Some(error::UNAUTHENTICATED | error::FORBIDDEN | error::APPROVAL_REQUIRED));
        if entry.meta.scope != Scope::Read || denied {
            self.record(&principal, entry.name, Some(entry.meta.scope), digest, code, started);
        }
        outcome
    }

    fn record(&self, principal: &Principal, method: &str, scope: Option<Scope>, digest: [u8; 32], code: Option<i32>, started: Instant) {
        if let Some(audit) = &self.audit {
            let ms = started.elapsed().as_millis() as u64;
            audit.record(&AuditEntry { principal: &principal.id, method, scope, digest, code, ms });
        }
    }
}
