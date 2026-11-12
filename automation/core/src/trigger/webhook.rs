//! `webhook` triggers: the proof a caller has the flow's secret, and what the request becomes in
//! `trigger` (docs/automation/webhooks.md). The HTTP route itself is the host's.

use super::Spec;
use crate::units::{parse_duration, parse_size};
use hmac::{Hmac, Mac};
use serde_json::{json, Map, Value};
use sha2::Sha256;

pub const SECRET_HEADER: &str = "x-cluaiz-hook-secret";
pub const SIGNATURE_HEADER: &str = "x-cluaiz-signature";
const DEFAULT_MAX_BODY: u64 = 1024 * 1024;
const DEFAULT_RESPOND_TIMEOUT_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Proof {
    /// The secret itself in `X-Cluaiz-Hook-Secret`
    Header,
    /// `sha256=<hex>`: the HMAC-SHA256 of the raw body, keyed with the secret
    Hmac { header: String },
}

#[derive(Debug, Clone)]
pub struct Hook {
    /// The `NAME` of `env:NAME`
    pub secret_env: String,
    pub proof: Proof,
    /// Upper-case HTTP methods
    pub methods: Vec<String>,
    /// `true` for `respond: result`: the answer waits for the run's outputs
    pub wait_for_result: bool,
    pub respond_timeout_ms: u64,
    pub max_body: u64,
}

impl Hook {
    pub fn of(spec: &Spec) -> Result<Hook, String> {
        let secret = spec.text("secret").unwrap_or_default();
        let secret_env = secret.strip_prefix("env:").filter(|n| !n.is_empty()).ok_or("a webhook's `secret` is env:NAME")?.to_string();
        let proof = match spec.text("signature") {
            Some("hmac-sha256") => Proof::Hmac { header: spec.text("signature_header").unwrap_or(SIGNATURE_HEADER).to_ascii_lowercase() },
            _ => Proof::Header,
        };
        let methods = match spec.field("methods").and_then(Value::as_array) {
            Some(list) => list.iter().filter_map(Value::as_str).map(str::to_ascii_uppercase).collect(),
            None => vec!["POST".to_string()],
        };
        Ok(Hook {
            secret_env,
            proof,
            methods,
            wait_for_result: spec.text("respond") == Some("result"),
            respond_timeout_ms: spec.text("respond_timeout").and_then(|d| parse_duration(d).ok()).unwrap_or(DEFAULT_RESPOND_TIMEOUT_MS).max(0) as u64,
            max_body: spec.text("max_body").and_then(|s| parse_size(s).ok()).unwrap_or(DEFAULT_MAX_BODY),
        })
    }

    /// Headers that prove the secret, removed from `trigger.headers`
    fn proof_headers(&self) -> [&str; 2] {
        match &self.proof {
            Proof::Header => [SECRET_HEADER, SECRET_HEADER],
            Proof::Hmac { header } => [header.as_str(), SECRET_HEADER],
        }
    }

    /// Whether the request proves the secret. `header` reads a request header by lower-case name.
    pub fn proves(&self, secret: &str, header: &dyn Fn(&str) -> Option<String>, body: &[u8]) -> bool {
        if secret.is_empty() {
            return false;
        }
        match &self.proof {
            Proof::Header => header(SECRET_HEADER).is_some_and(|given| same(given.as_bytes(), secret.as_bytes())),
            Proof::Hmac { header: name } => {
                let Some(given) = header(name) else { return false };
                let hex = given.trim().strip_prefix("sha256=").unwrap_or(given.trim()).to_ascii_lowercase();
                let expected: String = hmac_sha256(secret.as_bytes(), body).iter().map(|b| format!("{b:02x}")).collect();
                same(hex.as_bytes(), expected.as_bytes())
            }
        }
    }
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else { return [0; 32] };
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// Equal, in a time that does not depend on where they differ
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// `a=1&b=x+y` as an object; a name given twice keeps the last value
pub fn form(text: &str) -> Value {
    let mut out = Map::new();
    for pair in text.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(percent_decode(name), json!(percent_decode(value)));
    }
    Value::Object(out)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Why a body cannot become `trigger.body`
#[derive(Debug, PartialEq)]
pub enum BodyError {
    /// Not text, or a type this release does not read (multipart)
    Unsupported(String),
    BadJson(String),
}

/// The body as `trigger.body`: JSON as JSON, form data as an object, other text as text
pub fn body(content_type: Option<&str>, raw: &[u8]) -> Result<Value, BodyError> {
    let media = content_type.unwrap_or_default().split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    if media.starts_with("multipart/") {
        return Err(BodyError::Unsupported("multipart bodies are not read yet in this release of Automation".into()));
    }
    let text = std::str::from_utf8(raw).map_err(|_| BodyError::Unsupported("binary bodies are not read yet in this release of Automation".into()))?;
    match media.as_str() {
        "application/json" => serde_json::from_str(text).map_err(|e| BodyError::BadJson(e.to_string())),
        m if m.ends_with("+json") => serde_json::from_str(text).map_err(|e| BodyError::BadJson(e.to_string())),
        "application/x-www-form-urlencoded" => Ok(form(text)),
        _ => Ok(Value::String(text.to_string())),
    }
}

/// What `trigger` holds for a request (before `kind`, `id` and `at` are added)
pub fn data(hook: &Hook, method: &str, headers: &[(String, String)], query: Option<&str>, body: Value, remote: Option<String>) -> Value {
    let hidden = hook.proof_headers();
    let mut kept = Map::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if !hidden.contains(&name.as_str()) && name != "authorization" && name != "cookie" {
            kept.insert(name, json!(value));
        }
    }
    json!({ "method": method, "headers": kept, "query": form(query.unwrap_or_default()), "body": body, "remote": remote })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(webhook: Value) -> Hook {
        Hook::of(&super::super::specs(&json!({ "on": [{ "webhook": webhook }] })).remove(0)).unwrap()
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        let mac: String = hmac_sha256(b"Jefe", b"what do ya want for nothing?").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(mac, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn proofs() {
        let plain = hook(json!({ "secret": "env:H" }));
        let given = |value: &'static str| move |name: &str| (name == SECRET_HEADER).then(|| value.to_string());
        assert!(plain.proves("s3cret", &given("s3cret"), b""));
        assert!(!plain.proves("s3cret", &given("s3cre"), b""));
        assert!(!plain.proves("", &given(""), b""), "no secret set: nobody gets in");
        let github = hook(json!({ "secret": "env:H", "signature": "hmac-sha256", "signature_header": "X-Hub-Signature-256" }));
        let body = br#"{"action":"opened"}"#;
        let signature = format!("sha256={}", hmac_sha256(b"k", body).iter().map(|b| format!("{b:02x}")).collect::<String>());
        let header = |name: &str| (name == "x-hub-signature-256").then(|| signature.clone());
        assert!(github.proves("k", &header, body));
        assert!(!github.proves("k", &header, br#"{"action":"closed"}"#));
        assert!(!github.proves("other", &header, body));
    }

    #[test]
    fn bodies_and_data() {
        assert_eq!(body(Some("application/json; charset=utf-8"), br#"{"a":1}"#).unwrap(), json!({ "a": 1 }));
        assert_eq!(body(Some("application/x-www-form-urlencoded"), b"name=Asha+K&city=Pune%20West&x=%zz").unwrap(), json!({ "name": "Asha K", "city": "Pune West", "x": "%zz" }));
        assert_eq!(body(Some("text/plain"), b"hi").unwrap(), json!("hi"));
        assert_eq!(body(None, b"").unwrap(), Value::Null);
        assert!(matches!(body(Some("application/json"), b"{"), Err(BodyError::BadJson(_))));
        assert!(matches!(body(Some("application/octet-stream"), &[0xff, 0xfe]), Err(BodyError::Unsupported(_))));
        let h = hook(json!({ "secret": "env:H" }));
        let headers = vec![("Content-Type".to_string(), "application/json".to_string()), ("X-Cluaiz-Hook-Secret".to_string(), "s".to_string())];
        let d = data(&h, "POST", &headers, Some("a=1"), json!({}), Some("10.0.0.2".into()));
        assert_eq!(d["headers"], json!({ "content-type": "application/json" }));
        assert_eq!(d["query"], json!({ "a": "1" }));
        assert_eq!(h.max_body, 1024 * 1024);
        assert_eq!(h.methods, vec!["POST"]);
    }
}
