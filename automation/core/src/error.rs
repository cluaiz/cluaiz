//! The error object of a failed step or run: `{ kind, message, step, details }`
//! (docs/automation/errors.md).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The error kinds Automation itself reports. A `fail` step may record a kind of its own, so
/// [`FlowError::kind`] is text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Network,
    RateLimit,
    Timeout,
    Provider,
    Model,
    Tool,
    InvalidInput,
    InvalidOutput,
    Expression,
    Permission,
    ApprovalRejected,
    NotFound,
    Conflict,
    Unavailable,
    Code,
    Limit,
    Cancelled,
    NotCallable,
    Failed,
}

impl Kind {
    pub const ALL: [Kind; 19] = [
        Kind::Network,
        Kind::RateLimit,
        Kind::Timeout,
        Kind::Provider,
        Kind::Model,
        Kind::Tool,
        Kind::InvalidInput,
        Kind::InvalidOutput,
        Kind::Expression,
        Kind::Permission,
        Kind::ApprovalRejected,
        Kind::NotFound,
        Kind::Conflict,
        Kind::Unavailable,
        Kind::Code,
        Kind::Limit,
        Kind::Cancelled,
        Kind::NotCallable,
        Kind::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Network => "network",
            Kind::RateLimit => "rate_limit",
            Kind::Timeout => "timeout",
            Kind::Provider => "provider",
            Kind::Model => "model",
            Kind::Tool => "tool",
            Kind::InvalidInput => "invalid_input",
            Kind::InvalidOutput => "invalid_output",
            Kind::Expression => "expression",
            Kind::Permission => "permission",
            Kind::ApprovalRejected => "approval_rejected",
            Kind::NotFound => "not_found",
            Kind::Conflict => "conflict",
            Kind::Unavailable => "unavailable",
            Kind::Code => "code",
            Kind::Limit => "limit",
            Kind::Cancelled => "cancelled",
            Kind::NotCallable => "not_callable",
            Kind::Failed => "failed",
        }
    }

    /// Retried when a step has `retry` without `on`
    pub fn retried_by_default(self) -> bool {
        matches!(self, Kind::Network | Kind::RateLimit | Kind::Timeout)
    }
}

/// Kinds that `retry: { on: [any] }` still never retries
pub const NEVER_RETRIED: [&str; 3] = ["cancelled", "permission", "approval_rejected"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowError {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub details: Value,
}

impl FlowError {
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self { kind: kind.as_str().to_string(), message: message.into(), step: None, details: Value::Null }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn at_step(mut self, step: &str) -> Self {
        if self.step.is_none() {
            self.step = Some(step.to_string());
        }
        self
    }

    pub fn is(&self, kind: Kind) -> bool {
        self.kind == kind.as_str()
    }

    /// A part of the format this release reads and checks, but does not run yet
    pub fn not_built(what: &str) -> Self {
        Self::new(Kind::Unavailable, format!("{what} is not built yet in this release of Automation"))
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for FlowError {}
