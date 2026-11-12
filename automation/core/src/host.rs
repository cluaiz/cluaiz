//! The only way out of Automation: what a run asks of the Engine. The Engine side implements
//! [`StepHost`] with the Engine's public doors (CEP methods, the chat pipeline), so every step is
//! checked by the Engine's scopes, folders and approvals and written to its audit log.

use crate::error::FlowError;
use async_trait::async_trait;
use serde_json::Value;

/// Which run is asking, and what it may do
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub flow: String,
    pub scope: String,
    pub run_id: String,
    /// The flow's `permissions` (as confirmed by the owner)
    pub permissions: Value,
    /// The folder of the flow's file, when known
    pub flow_dir: Option<std::path::PathBuf>,
}

/// What Automation knows about something a step can run
#[derive(Debug, Clone, Default)]
pub struct NodeInfo {
    /// What it does, in its own words (an agent's model reads it)
    pub description: Option<String>,
    /// Its input schema, when it has one
    pub input_schema: Option<Value>,
    /// The CEP scope a method needs: read, write, exec or admin
    pub scope: Option<String>,
    /// Reads only: it runs for real in a dry run
    pub read_only: bool,
    /// Reaches the network (a missing `retry` is a warning)
    pub network: bool,
    /// Sends something people receive (mail, posts, payments): `x-cluaiz-sends`
    pub sends: bool,
    /// Runs as a task; its result is waited for
    pub task: bool,
}

/// What is installed and callable here; validation reads it to name unknown nodes
pub trait Catalog: Send + Sync {
    /// `mcp:<server>/<tool>`, `plugin:<name>[/<fn>]`, `skill:<name>`
    fn tool(&self, name: &str) -> Option<NodeInfo>;
    /// A CEP method
    fn method(&self, name: &str) -> Option<NodeInfo>;
    /// Every tool and method name, for suggestions
    fn names(&self) -> Vec<String>;
    /// `Some(false)` when the model is known not to be installed; `None` when it cannot be told
    fn model_installed(&self, model: &str) -> Option<bool>;
}

/// No catalog: nodes are not checked (checks without an Engine, tests)
pub struct NoCatalog;

impl Catalog for NoCatalog {
    fn tool(&self, _: &str) -> Option<NodeInfo> {
        None
    }
    fn method(&self, _: &str) -> Option<NodeInfo> {
        None
    }
    fn names(&self) -> Vec<String> {
        Vec::new()
    }
    fn model_installed(&self, _: &str) -> Option<bool> {
        None
    }
}

/// One `ask` request, as the chat pipeline receives it. An `agent` step sends the whole
/// conversation in `messages` (OpenAI chat messages) and its tools in `tools`.
#[derive(Debug, Clone, Default)]
pub struct AskRequest {
    pub model: Option<String>,
    pub system: String,
    pub user: String,
    /// When not empty, the conversation to send instead of `system` and `user`
    pub messages: Vec<Value>,
    /// OpenAI function tools the model may call; its calls come back in `tool_calls`
    pub tools: Vec<Value>,
    /// `response_format` with this schema
    pub output_schema: Option<Value>,
    pub think: Option<String>,
    pub effort: Option<String>,
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct AskAnswer {
    pub text: String,
    /// OpenAI `tool_calls` of the answer: `{ id, function: { name, arguments } }`
    pub tool_calls: Vec<Value>,
    pub thinking: Option<String>,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// A yes-or-no question to the owner (`approve`, `approve_before`)
#[derive(Debug, Clone, Default)]
pub struct OwnerQuestion {
    pub question: String,
    /// Where in the flow it is asked (for the owner's list)
    pub target: String,
    /// What the owner should see to decide
    pub details: Value,
    /// How long the question stays open; `None` is the Engine's usual time
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerAnswer {
    /// `None`: no answer in time
    pub approved: Option<bool>,
    pub note: Option<String>,
}

/// Something that happened: an Engine event, or a flow's `emit`
#[derive(Debug, Clone)]
pub struct HostEvent {
    pub name: String,
    pub data: Value,
    /// `{ flow, run_id }` for an `emit`; `null` for an Engine event
    pub from: Value,
}

/// A `run` step: another flow
#[derive(Debug, Clone)]
pub struct FlowCall {
    pub name: String,
    pub scope: Option<String>,
    pub inputs: Value,
    /// Wait for its outputs, or only start it
    pub wait: bool,
    /// `{ flow, scope, run_id, depth }` of the run that asks
    pub parent: Value,
    /// The other run has no side effects either
    pub dry_run: bool,
}

/// A `code` step: a WebAssembly module
#[derive(Debug, Clone)]
pub struct CodeCall {
    /// A `.wasm` file next to the flow file, or `plugin:<name>`
    pub module: String,
    pub function: String,
    pub input: Value,
    pub memory_bytes: u64,
    pub fuel: u64,
    /// Hosts `http_fetch` may reach (`permissions.network`)
    pub network: Vec<String>,
}

#[async_trait]
pub trait StepHost: Send + Sync {
    /// Runs a CEP method as the run's principal. A method that runs as a task is waited for and
    /// its result returned.
    async fn call(&self, run: &RunInfo, method: &str, params: Value) -> Result<Value, FlowError>;

    /// One answer from a model through the chat pipeline (installed or a provider's)
    async fn ask(&self, run: &RunInfo, request: AskRequest) -> Result<AskAnswer, FlowError>;

    /// A question to the owner, as an approval
    async fn approval(&self, run: &RunInfo, question: OwnerQuestion) -> Result<OwnerAnswer, FlowError>;

    /// A yes-or-no question with the Engine's usual time: `false` is a no or no answer
    async fn ask_owner(&self, run: &RunInfo, question: &str, target: &str) -> Result<bool, FlowError> {
        let asked = OwnerQuestion { question: question.to_string(), target: target.to_string(), ..Default::default() };
        Ok(self.approval(run, asked).await?.approved == Some(true))
    }

    fn catalog(&self) -> &dyn Catalog;

    /// Sends an event to other flows and to waiting runs; how many flows listen for it. `hops` is
    /// how many events led to this one (a flow started by an event that sends another…).
    async fn emit(&self, _run: &RunInfo, _name: &str, _data: Value, _hops: u64) -> Result<u64, FlowError> {
        Err(FlowError::new(crate::Kind::Unavailable, "events cannot be sent here"))
    }

    /// Events from now on, for `wait` with `event`
    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<HostEvent>> {
        None
    }

    /// Waits for a request to the run's own address (`run.webhook_url`); `None` after `timeout_ms`
    async fn wait_webhook(&self, _run: &RunInfo, _token: &str, _timeout_ms: u64) -> Result<Option<Value>, FlowError> {
        Err(FlowError::new(crate::Kind::Unavailable, "run addresses are not served here"))
    }

    /// The run's own address, when the host serves one
    fn webhook_url(&self, _run_id: &str, _token: &str) -> Option<String> {
        None
    }

    /// Runs another flow; its outputs (or `{ run_id }` when not waiting)
    async fn run_flow(&self, _run: &RunInfo, _call: FlowCall) -> Result<Value, FlowError> {
        Err(FlowError::new(crate::Kind::Unavailable, "other flows cannot be run here"))
    }

    /// Runs a WebAssembly module's function with JSON in and out
    async fn code(&self, _run: &RunInfo, _call: CodeCall) -> Result<Value, FlowError> {
        Err(FlowError::new(crate::Kind::Unavailable, "WebAssembly cannot be run here"))
    }
}
