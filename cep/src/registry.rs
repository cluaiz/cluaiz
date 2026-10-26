//! The method table. Dispatch, `rpc.discover`, the MCP tool list and the docs all read it, so a
//! method is described once: its Rust params type is its schema.

use crate::approval::{params_digest, ApprovalStore};
use crate::error::CepError;
use crate::principal::{OnDenied, Principal, Scope};
use crate::task::TaskStore;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// Whether a method answers directly or returns a task handle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Execution {
    Sync,
    Task,
}

/// Params of a method that takes none: any field is an error
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

/// What discovery and the policy know about a method
#[derive(Debug, Clone)]
pub struct Meta {
    pub summary: &'static str,
    pub scope: Scope,
    pub execution: Execution,
    pub destructive: bool,
    pub idempotent: bool,
    /// May be listed as an MCP tool (when the MCP token has the scope)
    pub mcp: bool,
    pub owner_only: bool,
    pub example_params: Value,
    pub example_result: Value,
    /// The method's own errors (the protocol's are the same for every method)
    pub errors: Vec<ErrorDoc>,
}

/// One error a method can answer, for discovery and the docs
#[derive(Debug, Clone, Serialize)]
pub struct ErrorDoc {
    pub code: i32,
    /// `data.type`
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// When it happens
    pub when: &'static str,
}

impl Meta {
    /// A sync, MCP-eligible method; set the example with `example`
    pub fn new(scope: Scope, summary: &'static str) -> Self {
        let idempotent = scope == Scope::Read;
        Self {
            summary,
            scope,
            execution: Execution::Sync,
            destructive: false,
            idempotent,
            mcp: true,
            owner_only: false,
            example_params: json!({}),
            example_result: Value::Null,
            errors: Vec::new(),
        }
    }

    pub fn example(mut self, params: Value, result: Value) -> Self {
        self.example_params = params;
        self.example_result = result;
        self
    }

    /// An error this method answers: `code`, `data.type`, and when
    pub fn error(mut self, code: i32, kind: &'static str, when: &'static str) -> Self {
        self.errors.push(ErrorDoc { code, kind, when });
        self
    }

    pub fn task(mut self) -> Self {
        self.execution = Execution::Task;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent = idempotent;
        self
    }

    pub fn no_mcp(mut self) -> Self {
        self.mcp = false;
        self
    }

    /// Only the owner (it opens UI on the host, or decides for others)
    pub fn owner_only(mut self) -> Self {
        self.owner_only = true;
        self.mcp = false;
        self
    }
}

/// Protocol state every call can reach
pub struct Shared {
    pub approvals: ApprovalStore,
    pub tasks: Arc<TaskStore>,
    /// Discovery entries, built once the registry is complete
    pub(crate) discovery: OnceLock<crate::discover::Discovery>,
}

/// One call, as its method sees it: who calls, and the protocol services. A clone (for work
/// that outlives the request, such as a task) is the same call: it shares its approval.
#[derive(Clone)]
pub struct Call {
    pub principal: Arc<Principal>,
    pub method: &'static str,
    pub digest: [u8; 32],
    pub approval: Option<String>,
    pub shared: Arc<Shared>,
    /// Set once this call's approval is used: one approval covers every check of the call
    /// (a copy with two outside paths, an audio run with an outside model and source)
    approved: Arc<AtomicBool>,
}

impl Call {
    pub(crate) fn new(principal: Arc<Principal>, method: &'static str, digest: [u8; 32], approval: Option<String>, shared: Arc<Shared>) -> Self {
        Self { principal, method, digest, approval, shared, approved: Arc::new(AtomicBool::new(false)) }
    }

    /// Goes on only with an approval made for exactly this call; otherwise asks the owner
    pub fn require_approval(&self, target: &str) -> Result<(), CepError> {
        if self.approved.load(Ordering::Acquire) {
            return Ok(());
        }
        let approvals = &self.shared.approvals;
        if let Some(id) = &self.approval {
            if approvals.consume(id, &self.principal.id, self.method, self.digest) {
                self.approved.store(true, Ordering::Release);
                return Ok(());
            }
        }
        let approval = approvals.request(&self.principal.id, self.method, self.digest, target);
        Err(CepError::approval_required(&approval.id, self.method, target, &approval.expires_at))
    }

    /// Roots check for a path the method already resolved (canonical): inside goes on; outside is
    /// `forbidden`, or an approval for a principal that asks the owner
    pub fn check_path(&self, resolved: &Path) -> Result<(), CepError> {
        if self.principal.within_roots(resolved) {
            return Ok(());
        }
        match self.principal.on_denied {
            OnDenied::Error => Err(CepError::forbidden("roots")),
            OnDenied::Ask => self.require_approval(&resolved.display().to_string()),
        }
    }

    /// Runs `work` as a task of this principal and returns `{"task": …}` at once
    pub fn start_task<F, Fut>(&self, work: F) -> Result<Value, CepError>
    where
        F: FnOnce(crate::task::TaskHandle) -> Fut,
        Fut: Future<Output = Result<Value, CepError>> + Send + 'static,
    {
        let view = self.shared.tasks.start(&self.principal.id, self.method, None, work)?;
        Ok(json!({ "task": view }))
    }
}

type BoxFuture = Pin<Box<dyn Future<Output = Result<Value, CepError>> + Send>>;
type Handler<Ctx> = Box<dyn Fn(Ctx, Call, Value) -> BoxFuture + Send + Sync>;

pub struct Method<Ctx> {
    pub name: &'static str,
    pub meta: Meta,
    pub params_schema: Value,
    pub result_schema: Value,
    pub(crate) handler: Handler<Ctx>,
}

pub struct Registry<Ctx> {
    methods: BTreeMap<&'static str, Method<Ctx>>,
}

/// `group.verb`, lowercase snake_case; `rpc.` is reserved for `rpc.discover`
pub fn valid_name(name: &str) -> bool {
    let word = |w: &str| {
        w.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match name.split_once('.') {
        Some(("rpc", verb)) => verb == "discover",
        Some((group, verb)) => word(group) && word(verb) && !verb.contains('.'),
        None => false,
    }
}

/// Turns a serde error at a path into the `errors` entry clients see
fn params_error(error: serde_path_to_error::Error<serde_json::Error>, example: &Value) -> CepError {
    let path = error.path().to_string();
    let message = error.inner().to_string();
    CepError::invalid_params(
        format!("Invalid params: {message}"),
        vec![json!({ "path": if path == "." { String::new() } else { path }, "message": message })],
        Some(example.clone()),
    )
}

impl<Ctx: Send + 'static> Registry<Ctx> {
    /// A registry with the protocol's own methods (`rpc.discover`, `tasks.*`)
    pub fn new() -> Self {
        let mut registry = Self { methods: BTreeMap::new() };
        crate::protocol::register(&mut registry);
        registry
    }

    /// Adds a method. Its params type gives the schema and the checks; a bad name, a duplicate or
    /// a missing example is a programming error and panics at start.
    pub fn add<P, R, F, Fut>(&mut self, name: &'static str, meta: Meta, method: F) -> &mut Self
    where
        P: DeserializeOwned + JsonSchema + 'static,
        R: Serialize + JsonSchema + 'static,
        F: Fn(Ctx, Call, P) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<R, CepError>> + Send + 'static,
    {
        assert!(valid_name(name), "CEP method name `{name}` is not group.verb");
        assert!(!self.methods.contains_key(name), "CEP method `{name}` registered twice");
        assert!(!meta.example_result.is_null(), "CEP method `{name}` has no example");
        assert!(
            serde_json::from_value::<P>(meta.example_params.clone()).is_ok(),
            "CEP method `{name}`: example params do not match its params type"
        );
        let example = meta.example_params.clone();
        let handler: Handler<Ctx> = Box::new(move |ctx, call, params| {
            let parsed = serde_path_to_error::deserialize::<_, P>(params).map_err(|e| params_error(e, &example));
            match parsed {
                Err(error) => Box::pin(async move { Err(error) }),
                Ok(params) => {
                    let future = method(ctx, call, params);
                    Box::pin(async move {
                        let result = future.await?;
                        serde_json::to_value(result).map_err(|e| {
                            tracing::error!("CEP result did not serialize: {e}");
                            CepError::internal("serialize")
                        })
                    })
                }
            }
        });
        let params_schema = serde_json::to_value(schemars::schema_for!(P)).unwrap_or_default();
        let result_schema = serde_json::to_value(schemars::schema_for!(R)).unwrap_or_default();
        self.methods.insert(name, Method { name, meta, params_schema, result_schema, handler });
        self
    }

    pub fn get(&self, name: &str) -> Option<&Method<Ctx>> {
        self.methods.get(name)
    }

    pub fn methods(&self) -> impl Iterator<Item = &Method<Ctx>> {
        self.methods.values()
    }

    /// Close names (edit distance up to 3), best first, and the methods of the same group
    pub fn suggestions(&self, name: &str) -> (Vec<String>, Vec<String>) {
        let mut close: Vec<(usize, &str)> = self
            .methods
            .keys()
            .map(|known| (strsim::levenshtein(name, known), *known))
            .filter(|(distance, _)| *distance <= 3)
            .collect();
        close.sort();
        let group = name.split('.').next().unwrap_or_default();
        let same_group = self.methods.keys().filter(|k| k.split('.').next() == Some(group)).map(|k| k.to_string()).collect();
        (close.into_iter().take(3).map(|(_, k)| k.to_string()).collect(), same_group)
    }
}

impl<Ctx: Send + 'static> Default for Registry<Ctx> {
    fn default() -> Self {
        Self::new()
    }
}

/// The digest used for approvals: the params without `_meta`
pub(crate) fn digest_of(params: &serde_json::Map<String, Value>) -> [u8; 32] {
    params_digest(&Value::Object(params.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Echo {
        text: String,
    }

    fn registry() -> Registry<()> {
        let mut registry = Registry::new();
        registry.add(
            "test.echo",
            Meta::new(Scope::Read, "Echo").example(json!({"text": "hi"}), json!("hi")),
            |_: (), _: Call, p: Echo| async move { Ok(p.text) },
        );
        registry
    }

    #[test]
    fn test_names() {
        for good in ["fs.read", "tools.clear_cache", "rpc.discover", "kv.v2"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in ["fs", "Fs.read", "fs.read.x", "fs-read.x", "rpc.other", "fs.", ".read", "fs.2read"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn test_duplicate_panics() {
        let mut registry = registry();
        registry.add("test.echo", Meta::new(Scope::Read, "x").example(json!({"text": "a"}), json!("a")), |_: (), _: Call, p: Echo| async move { Ok(p.text) });
    }

    #[test]
    #[should_panic(expected = "has no example")]
    fn test_missing_example_panics() {
        let mut registry: Registry<()> = Registry::new();
        registry.add("test.bare", Meta::new(Scope::Read, "x"), |_: (), _: Call, _: NoParams| async move { Ok(1) });
    }

    #[test]
    fn test_suggestions_and_schema() {
        let registry = registry();
        let (close, group) = registry.suggestions("test.ecko");
        assert_eq!((close, group), (vec!["test.echo".to_string()], vec!["test.echo".to_string()]));
        let schema = &registry.get("test.echo").unwrap().params_schema;
        assert_eq!(schema["properties"]["text"]["type"], "string");
        assert_eq!(schema["additionalProperties"], false);
    }
}
