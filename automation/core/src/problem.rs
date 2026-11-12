//! A problem `flows.validate` reports (docs/automation/validation.md#the-problem-object).

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    pub severity: Severity,
    pub code: &'static str,
    /// A JSON Pointer into the flow, the same for JSON and YAML
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub fix: Value,
}

impl Problem {
    pub fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, code, path: path.into(), line: None, column: None, message: message.into(), fix: Value::Null }
    }

    pub fn warning(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, ..Self::error(code, path, message) }
    }

    pub fn fix(mut self, fix: Value) -> Self {
        self.fix = fix;
        self
    }

    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// The closest of `known` to `name` (one to three edits), for `fix`
pub fn closest<'a>(name: &str, known: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    known
        .into_iter()
        .map(|k| (strsim::levenshtein(name, k), k))
        .filter(|(d, _)| *d > 0 && *d <= 3 && *d < name.len().max(2))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// One JSON Pointer token
pub fn pointer(base: &str, token: impl std::fmt::Display) -> String {
    format!("{base}/{}", token.to_string().replace('~', "~0").replace('/', "~1"))
}
