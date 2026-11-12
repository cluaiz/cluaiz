//! A provider's failures in plain words, and what kind each is: the kind decides whether a
//! request is sent again (`retry.rs`), made smaller, or the error goes to the person.

use cluaiz_core::providers::{redact, Target};
use serde_json::Value;

/// The words every context overflow carries, from the Engine and from a provider
pub const CONTEXT_FULL: &str = "exceeds the available context";

/// What went wrong at a provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The key was refused (401, 403)
    Key,
    /// No such model (404)
    NoModel,
    /// Too many requests for now (429): worth waiting for
    RateLimited,
    /// A spending cap, quota or billing limit: waiting does not help
    Quota,
    /// The request does not fit the model's context
    ContextFull,
    /// The provider is busy or failed on its side (5xx, 529, "overloaded"): worth one more try
    Busy,
    /// Anything else (a request the provider does not take, and the like)
    Other,
}

/// The kind of a failed answer, from its status and body
pub fn classify(status: u16, body: &str) -> Failure {
    let lower = body.to_lowercase();
    // A limit per minute or day passes; a spending cap or an empty balance does not. Words are
    // matched in the message, never in links (Groq's per-minute limit links to its billing page).
    let words: String = lower
        .split_whitespace()
        .filter(|w| !w.contains("://"))
        .collect::<Vec<_>>()
        .join(" ");
    let passes = [
        "per minute",
        "per day",
        "per second",
        "try again in",
        "(tpm)",
        "(rpm)",
        "(tpd)",
        "(rpd)",
    ]
    .iter()
    .any(|w| words.contains(w));
    let quota = !passes
        && [
            "spending cap",
            "quota",
            "insufficient balance",
            "credit balance",
            "billing hard limit",
            "payment required",
        ]
        .iter()
        .any(|w| words.contains(w));
    match status {
        401 | 403 => Failure::Key,
        404 => Failure::NoModel,
        402 => Failure::Quota,
        429 if quota => Failure::Quota,
        429 => Failure::RateLimited,
        400 | 413 if context_full(&lower) => Failure::ContextFull,
        500..=599 => Failure::Busy,
        _ if lower.contains("overloaded") => Failure::Busy,
        _ => Failure::Other,
    }
}

fn context_full(lower: &str) -> bool {
    lower.contains("context")
        || lower.contains("too long")
        || lower.contains("too large")
        || lower.contains("reduce the length")
        || lower.contains("maximum length")
}

/// A failed provider answer in plain words. A context overflow says "exceeds the available
/// context", as the Engine's own error does, so callers that make room for it work the same.
pub fn error_message(target: &Target, status: u16, body: &str, key: Option<&str>) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            // Some providers answer with a list of errors (Gemini)
            let v = v
                .as_array()
                .and_then(|list| list.first().cloned())
                .unwrap_or(v);
            let error = v.get("error")?;
            error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| error.as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| body.chars().take(400).collect());
    let detail = redact(detail.trim(), key);
    let name = target.full_name();
    let what = match classify(status, &detail) {
        // Signed in, but the account may not use this model (AWS and Google say "not authorized")
        Failure::Key if detail.contains("not authorized to perform") || detail.contains("PERMISSION_DENIED") => {
            format!("{} accepted the sign-in but this account may not use '{}' ({status}); give it access at the provider", target.provider, target.model)
        }
        Failure::Key => format!("{} refused the key ({status}); check it with `cluaiz provider test {}`", target.provider, target.provider),
        // A provider can list a model it no longer serves to this key (NVIDIA lists retired ones)
        Failure::NoModel => format!("{} does not serve the model '{}' to this key ({status}), even when its model list shows it; pick another from `cluaiz provider models {}`", target.provider, target.model, target.provider),
        Failure::RateLimited => format!("{} is rate limiting this key ({status}); wait, or use another model", target.provider),
        Failure::Quota => format!("{} stopped this key at its spending or quota limit ({status}); raise the limit at the provider, or use another model", target.provider),
        Failure::ContextFull => format!("the request {CONTEXT_FULL} of {name} ({status})"),
        Failure::Busy => format!("{} is busy or failed on its side ({status}); try again shortly", target.provider),
        Failure::Other => format!("{} answered {status}", target.provider),
    };
    if detail.is_empty() {
        what
    } else {
        format!("{what}: {detail}")
    }
}

/// A connection error without the key (reqwest names only the URL)
pub fn send_error(target: &Target, error: &reqwest::Error, key: Option<&str>) -> String {
    if error.is_timeout() {
        return format!(
            "{} did not answer in time at {}: the model '{}' may be busy or slow there; try again shortly",
            target.provider, target.config.base_url, target.model
        );
    }
    redact(
        &format!(
            "{} did not answer at {}: {error}",
            target.provider, target.config.base_url
        ),
        key,
    )
}

/// The provider refused a call because the function was not in `tools`
pub(super) fn tool_call_refused(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("tool_use_failed")
        || lower.contains("tool call validation failed")
        || lower.contains("not in request.tools")
}

/// The model cannot stop thinking and the provider refused the request that asked it to
/// ("Reasoning is mandatory for this endpoint and cannot be disabled")
pub(super) fn thinking_required(status: u16, body: &str) -> bool {
    let lower = body.to_lowercase();
    matches!(status, 400 | 422)
        && (lower.contains("reasoning") || lower.contains("thinking"))
        && [
            "mandatory",
            "cannot be disabled",
            "can't be disabled",
            "can not be disabled",
            "cannot be turned off",
            "cannot disable",
            "must be enabled",
            "always on",
            "is required",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// The call(s) the model made, from the refusal (`failed_generation`), in the Engine's
/// `<tool_call>` form; None when the refusal does not carry them as JSON
pub(super) fn refused_calls(body: &str) -> Option<String> {
    let error: Value = serde_json::from_str(body).ok()?;
    let failed = error.pointer("/error/failed_generation")?.as_str()?.trim();
    let start = failed.find(['{', '['])?;
    let value: Value = serde_json::from_str(&failed[start..]).ok()?;
    let calls: Vec<Value> = match value {
        Value::Array(calls) => calls,
        call => vec![call],
    };
    let text: Vec<String> = calls
        .iter()
        .filter_map(|call| {
            let call = call.get("function").unwrap_or(call);
            let name = call.get("name")?.as_str()?;
            let arguments = match call.get("arguments").or_else(|| call.get("parameters")) {
                Some(Value::String(text)) => text.clone(),
                Some(arguments) => arguments.to_string(),
                None => "{}".into(),
            };
            Some(cluaiz_core::tool_calls::canonical_call(name, &arguments))
        })
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

/// Words a provider uses when it does not take a field
const REFUSAL_WORDS: [&str; 12] = [
    "unknown",
    "unrecognized",
    "not supported",
    "unsupported",
    "not allowed",
    "not permitted",
    "extra",
    "additional",
    "unexpected",
    "invalid",
    "does not match",
    "not available",
];

/// Which of the optional fields the Engine sent (`sent`) the provider says it does not take, from
/// its 400 or 422 answer: such a field is left out and the request sent again
pub(super) fn refused_field<'a>(status: u16, body: &str, sent: &[&'a str]) -> Option<&'a str> {
    if !matches!(status, 400 | 422) {
        return None;
    }
    let lower = body.to_lowercase();
    if !REFUSAL_WORDS.iter().any(|w| lower.contains(w)) {
        return None;
    }
    // The longest name first: `max_completion_tokens` before `max_tokens`
    let mut names: Vec<&'a str> = sent.to_vec();
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    names.into_iter().find(|name| lower.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;

    fn target() -> Target {
        Target {
            provider: "groq".into(),
            config: Provider::new("https://api.groq.com/openai/v1"),
            model: "qwen/qwen3-32b".into(),
        }
    }

    #[test]
    fn errors_name_the_cause_and_never_the_key() {
        let key = "gsk_secretsecretsecret1234";
        let t = target();
        let refused = error_message(
            &t,
            401,
            &format!(r#"{{"error":{{"message":"Invalid API Key {key}"}}}}"#),
            Some(key),
        );
        assert!(refused.contains("refused the key") && !refused.contains("secretsecret"));
        let denied = error_message(&t, 403, r#"{"message":"User: arn:aws:iam::1:user/u is not authorized to perform: bedrock:InvokeModel on resource: x"}"#, None);
        assert!(denied.contains("accepted the sign-in") && denied.contains("bedrock:InvokeModel"), "{denied}");
        let long = error_message(
            &t,
            400,
            r#"{"error":{"message":"Please reduce the length of the messages or completion."}}"#,
            None,
        );
        assert!(long.contains("exceeds the available context"));
        assert!(error_message(&t, 429, "slow down", None).contains("rate limiting"));
        assert!(error_message(&t, 500, "", None).contains("busy or failed on its side (500)"));
        assert!(error_message(&t, 418, "", None).ends_with("answered 418"));
    }

    #[test]
    fn a_refused_call_is_passed_on_to_the_engine() {
        let body = r#"{"error":{"message":"tool call validation failed: attempted to call tool 'math_evaluate' which was not in request.tools","type":"invalid_request_error","code":"tool_use_failed","failed_generation":"{\"name\": \"math_evaluate\", \"arguments\": {\"expression\": \"2**40\"}}"}}"#;
        assert!(tool_call_refused(body));
        let calls = refused_calls(body).unwrap();
        let (_, found) = cluaiz_core::tool_calls::split_calls(&calls);
        assert_eq!(
            found,
            vec![(
                "math_evaluate".to_string(),
                "{\"expression\":\"2**40\"}".to_string()
            )]
        );
        assert!(refused_calls(r#"{"error":{"message":"tool call validation failed","failed_generation":"I will call it"}}"#).is_none());
        assert!(!tool_call_refused(
            r#"{"error":{"message":"Invalid API Key"}}"#
        ));
    }

    #[test]
    fn each_failure_has_its_kind() {
        assert_eq!(
            classify(429, "Rate limit reached for model"),
            Failure::RateLimited
        );
        assert_eq!(
            classify(429, "Your project has exceeded its monthly spending cap"),
            Failure::Quota
        );
        // Groq's per-minute limit links to its billing page: still a limit that passes
        let groq = "Rate limit reached for model `openai/gpt-oss-20b` in organization `org_1` service tier `on_demand` on tokens per minute (TPM): Limit 8000, Used 6220, Requested 2883. Please try again in 8.27s. Need more tokens? Upgrade to Dev Tier today at https://console.groq.com/settings/billing";
        assert_eq!(classify(429, groq), Failure::RateLimited);
        assert_eq!(
            classify(
                429,
                "You exceeded your current quota, please check your plan and billing details."
            ),
            Failure::Quota
        );
        assert_eq!(classify(402, ""), Failure::Quota);
        assert_eq!(classify(503, "upstream"), Failure::Busy);
        assert_eq!(classify(529, "Overloaded"), Failure::Busy);
        assert_eq!(
            classify(400, "This model's maximum context length is 8192 tokens"),
            Failure::ContextFull
        );
        assert_eq!(classify(401, ""), Failure::Key);
        assert_eq!(classify(400, "bad"), Failure::Other);
        // Gemini answers with a list of errors
        let gemini = r#"[{"error":{"code":429,"message":"Your project has exceeded its monthly spending cap.","status":"RESOURCE_EXHAUSTED"}}]"#;
        assert!(error_message(&target(), 429, gemini, None).contains("spending or quota limit"));
    }

    #[test]
    fn a_refused_optional_field_is_named() {
        let sent = [
            "stream_options",
            "reasoning_effort",
            "max_completion_tokens",
            "temperature",
        ];
        assert_eq!(
            refused_field(
                400,
                r#"{"error":{"message":"Unrecognized request argument supplied: reasoning_effort"}}"#,
                &sent
            ),
            Some("reasoning_effort")
        );
        assert_eq!(
            refused_field(
                400,
                "Unsupported parameter: 'temperature' is not supported with this model.",
                &sent
            ),
            Some("temperature")
        );
        assert_eq!(
            refused_field(422, "extra fields not permitted: stream_options", &sent),
            Some("stream_options")
        );
        assert_eq!(
            refused_field(400, "Unsupported value: max_completion_tokens", &sent),
            Some("max_completion_tokens")
        );
        // Not a refusal, another status, or a field the Engine did not send
        assert_eq!(
            refused_field(400, "temperature must be at most 2", &sent),
            None
        );
        assert_eq!(
            refused_field(500, "unknown field reasoning_effort", &sent),
            None
        );
        assert_eq!(refused_field(400, "unknown field logit_bias", &sent), None);
    }

    #[test]
    fn a_model_that_cannot_stop_thinking_is_known() {
        let openrouter = r#"{"error":{"message":"Reasoning is mandatory for this endpoint and cannot be disabled.","code":400}}"#;
        assert!(thinking_required(400, openrouter));
        assert!(thinking_required(422, "thinking cannot be turned off for this model"));
        // Another status, or a refusal about something else
        assert!(!thinking_required(500, openrouter));
        assert!(!thinking_required(400, "temperature is required"));
        assert!(!thinking_required(400, "reasoning_effort must be one of low, medium, high"));
    }
}
