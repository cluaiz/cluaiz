//! Execution records: what a tool call touched, with secrets taken out before it is logged.

use serde_json::Value;

/// Argument names that hold a file or folder path
const PATH_KEYS: [&str; 5] = ["path", "paths", "source", "destination", "file"];
/// Argument names whose values are secrets (matched case-insensitively, also as part of a name)
const SECRET_KEYS: [&str; 9] = ["password", "passwd", "secret", "token", "api_key", "apikey", "authorization", "private_key", "credential"];
/// Prefixes of well-known secret strings (API keys and tokens)
const SECRET_PREFIXES: [&str; 7] = ["sk-", "ghp_", "gho_", "github_pat_", "xoxb-", "xoxp-", "AKIA"];

pub const REDACTED: &str = "[redacted]";

/// The JSON of a `<tool_call>…</tool_call>` block
pub fn call_json(call_xml: &str) -> Option<Value> {
    serde_json::from_str(call_xml.trim().trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim()).ok()
}

/// Files and folders a call names in its arguments
pub fn files_touched(arguments: &Value) -> Vec<String> {
    let arguments = match arguments {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
        other => other.clone(),
    };
    let mut files = Vec::new();
    for key in PATH_KEYS {
        match arguments.get(key) {
            Some(Value::String(path)) if !path.is_empty() => files.push(path.clone()),
            Some(Value::Array(paths)) => files.extend(paths.iter().filter_map(Value::as_str).map(str::to_string)),
            _ => {}
        }
    }
    files.dedup();
    files
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_lowercase().replace('-', "_");
    SECRET_KEYS.iter().any(|secret| key.contains(secret))
}

fn looks_like_secret(text: &str) -> bool {
    let text = text.trim();
    text.len() >= 16 && SECRET_PREFIXES.iter().any(|prefix| text.starts_with(prefix)) && !text.contains(char::is_whitespace)
}

/// The value with secrets replaced: values of secret-named keys, and strings that look like keys
pub fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, field)| {
                    let field = if is_secret_key(key) && !field.is_null() { Value::String(REDACTED.into()) } else { redact(field) };
                    (key.clone(), field)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        Value::String(text) if looks_like_secret(text) => Value::String(REDACTED.into()),
        // Arguments often arrive as a JSON string
        Value::String(text) if text.trim_start().starts_with('{') => match serde_json::from_str::<Value>(text) {
            Ok(inner @ Value::Object(_)) => Value::String(redact(&inner).to_string()),
            _ => value.clone(),
        },
        other => other.clone(),
    }
}

/// A `<tool_call>` block with its arguments redacted
pub fn redact_call(call_xml: &str) -> String {
    match call_json(call_xml) {
        Some(call) => format!("<tool_call>\n{}\n</tool_call>", redact(&call)),
        None => call_xml.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_files_from_known_argument_names() {
        assert_eq!(files_touched(&json!({"path": "notes.txt", "content": "x"})), ["notes.txt"]);
        assert_eq!(files_touched(&json!("{\"source\": \"a.txt\", \"destination\": \"b.txt\"}")), ["a.txt", "b.txt"]);
        assert_eq!(files_touched(&json!({"paths": ["x.rs", "y.rs"]})), ["x.rs", "y.rs"]);
        assert!(files_touched(&json!({"query": "rust"})).is_empty());
    }

    #[test]
    fn test_secrets_are_redacted() {
        let call = r#"<tool_call>{"name": "http", "arguments": {"url": "https://x.dev", "api_key": "abc", "headers": {"Authorization": "Bearer t"}, "note": "sk-1234567890abcdefXYZ"}}</tool_call>"#;
        let redacted = redact_call(call);
        assert!(!redacted.contains("abc") && !redacted.contains("Bearer t") && !redacted.contains("sk-1234567890"));
        assert!(redacted.contains("https://x.dev"));
        let nested = redact(&json!({"arguments": "{\"password\": \"hunter2\", \"user\": \"me\"}"}));
        assert!(!nested.to_string().contains("hunter2") && nested.to_string().contains("me"));
    }
}
