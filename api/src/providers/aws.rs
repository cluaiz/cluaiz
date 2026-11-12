//! AWS credentials and Signature Version 4 for Amazon Bedrock (`auth: "aws"`), as the AWS tools do:
//! the access key in `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` (with `AWS_SESSION_TOKEN`), else
//! the profile `AWS_PROFILE` (or `default`) in `~/.aws/credentials`, else what
//! `aws configure export-credentials` gives (single sign-on and roles). Each request is signed
//! with its method, URL, headers and body; the region and the service are told from the host
//! (`bedrock-runtime.<region>.amazonaws.com` signs as `bedrock`, `bedrock-mantle.<region>.api.aws`
//! as `bedrock-mantle`). No key or signature is ever logged.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use ring::{digest, hmac};
use serde_json::Value;

/// Credentials from the AWS CLI are renewed this long before they end
const RENEW_BEFORE: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Credentials {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

static FROM_CLI: Mutex<Option<(Credentials, Instant)>> = Mutex::new(None);

/// The credentials to sign with: the variables, else the credentials file, else the AWS CLI
pub(super) async fn credentials() -> Result<Credentials, String> {
    if let Some(found) = from_env().or_else(from_file) {
        return Ok(found);
    }
    if let Ok(cached) = FROM_CLI.lock() {
        if let Some((found, _)) = cached
            .as_ref()
            .filter(|(_, until)| Instant::now() + RENEW_BEFORE < *until)
        {
            return Ok(found.clone());
        }
    }
    let (found, lasts) = from_cli().await?;
    if let Ok(mut cached) = FROM_CLI.lock() {
        *cached = Some((found.clone(), Instant::now() + lasts));
    }
    Ok(found)
}

fn from_env() -> Option<Credentials> {
    let value = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Some(Credentials {
        access_key: value("AWS_ACCESS_KEY_ID")?,
        secret_key: value("AWS_SECRET_ACCESS_KEY")?,
        session_token: value("AWS_SESSION_TOKEN"),
    })
}

fn from_file() -> Option<Credentials> {
    let path = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".aws").join("credentials")))?;
    let text = std::fs::read_to_string(path).ok()?;
    let profile = std::env::var("AWS_PROFILE")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "default".into());
    profile_of(&text, profile.trim())
}

/// One profile of an AWS credentials file (INI: `[name]`, then `key = value` lines)
fn profile_of(text: &str, profile: &str) -> Option<Credentials> {
    let (mut inside, mut access, mut secret, mut token) = (false, None, None, None);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            inside = line[1..line.len() - 1].trim() == profile;
        } else if inside {
            if let Some((key, value)) = line.split_once('=') {
                let value = Some(value.trim().to_string()).filter(|v| !v.is_empty());
                match key.trim().to_ascii_lowercase().as_str() {
                    "aws_access_key_id" => access = value,
                    "aws_secret_access_key" => secret = value,
                    "aws_session_token" => token = value,
                    _ => {}
                }
            }
        }
    }
    Some(Credentials {
        access_key: access?,
        secret_key: secret?,
        session_token: token,
    })
}

/// `aws configure export-credentials --format process`: single sign-on, roles, and anything else
/// the AWS CLI knows how to sign in with
async fn from_cli() -> Result<(Credentials, Duration), String> {
    let output = tokio::process::Command::new("aws")
        .args(["configure", "export-credentials", "--format", "process"])
        .output()
        .await
        .map_err(|_| "no AWS credentials: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, run `aws configure`, or use a Bedrock API key".to_string())?;
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let field = |name: &str| {
        answer[name]
            .as_str()
            .map(str::to_string)
            .filter(|v| !v.is_empty())
    };
    let (Some(access_key), Some(secret_key)) = (field("AccessKeyId"), field("SecretAccessKey"))
    else {
        return Err("the AWS CLI gave no credentials: run `aws configure` (or `aws sso login`), or use a Bedrock API key".into());
    };
    let lasts = field("Expiration")
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(&e).ok())
        .and_then(|end| {
            (end.with_timezone(&chrono::Utc) - chrono::Utc::now())
                .to_std()
                .ok()
        })
        .unwrap_or(Duration::from_secs(3000));
    Ok((
        Credentials {
            access_key,
            secret_key,
            session_token: field("SessionToken"),
        },
        lasts,
    ))
}

/// The region and the service a Bedrock host signs for
pub(super) fn scope_of(host: &str) -> Option<(String, String)> {
    let labels: Vec<&str> = host.split('.').collect();
    let known = host.ends_with(".amazonaws.com") || host.ends_with(".api.aws");
    if !known || labels.len() < 4 {
        return None;
    }
    let service = match labels[0] {
        "bedrock-runtime"
        | "bedrock-runtime-fips"
        | "bedrock"
        | "bedrock-fips"
        | "bedrock-agent-runtime" => "bedrock",
        other => other,
    };
    Some((labels[1].to_string(), service.to_string()))
}

/// Signs `request` in place: `X-Amz-Date`, `X-Amz-Security-Token` with a session token,
/// `X-Amz-Content-Sha256` when the body cannot be read in full, and `Authorization`
pub(super) fn sign(
    request: &mut reqwest::Request,
    credentials: &Credentials,
    now: SystemTime,
) -> Result<(), String> {
    let url = request.url().clone();
    let host = url.host_str().ok_or("the URL has no host")?.to_string();
    let (region, service) = scope_of(&host).ok_or_else(|| {
        format!("{host} is not an AWS host: the region to sign for cannot be told")
    })?;
    let amz_date = chrono::DateTime::<chrono::Utc>::from(now)
        .format("%Y%m%dT%H%M%SZ")
        .to_string();
    let payload = match request.body().and_then(|b| b.as_bytes()) {
        Some(bytes) => hex(digest::digest(&digest::SHA256, bytes).as_ref()),
        None if request.body().is_none() => hex(digest::digest(&digest::SHA256, b"").as_ref()),
        None => "UNSIGNED-PAYLOAD".to_string(),
    };
    let headers = request.headers_mut();
    headers.insert("x-amz-date", amz_date.parse().map_err(|_| "bad date")?);
    if let Some(token) = &credentials.session_token {
        headers.insert(
            "x-amz-security-token",
            token
                .parse()
                .map_err(|_| "the AWS session token is not a header value")?,
        );
    }
    if payload == "UNSIGNED-PAYLOAD" {
        headers.insert(
            "x-amz-content-sha256",
            payload.parse().map_err(|_| "bad payload hash")?,
        );
    }
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    let mut signed: Vec<(String, String)> = vec![("host".into(), format!("{host}{port}"))];
    for (name, value) in headers.iter() {
        let name = name.as_str().to_ascii_lowercase();
        if name == "content-type" || name.starts_with("x-amz-") {
            signed.push((name, value.to_str().unwrap_or_default().to_string()));
        }
    }
    let parts = Parts {
        method: request.method().as_str(),
        url: &url,
        headers: signed,
        payload_hash: &payload,
        region: &region,
        service: &service,
        amz_date: &amz_date,
    };
    let authorization = authorization(&parts, credentials);
    request.headers_mut().insert(
        reqwest::header::AUTHORIZATION,
        authorization.parse().map_err(|_| "bad signature header")?,
    );
    Ok(())
}

/// What a signature is made of
struct Parts<'a> {
    method: &'a str,
    url: &'a reqwest::Url,
    /// Lowercase names; `host` among them
    headers: Vec<(String, String)>,
    payload_hash: &'a str,
    region: &'a str,
    service: &'a str,
    /// `20150830T123600Z`
    amz_date: &'a str,
}

/// The `Authorization` header value (AWS4-HMAC-SHA256)
fn authorization(parts: &Parts, credentials: &Credentials) -> String {
    let mut headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .map(|(n, v)| {
            (
                n.to_ascii_lowercase(),
                v.split_whitespace().collect::<Vec<_>>().join(" "),
            )
        })
        .collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(n, v)| format!("{n}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(";");

    // Services other than S3 encode the (already encoded) path once more
    let path = if parts.url.path().is_empty() {
        "/"
    } else {
        parts.url.path()
    };
    let canonical_uri = encode(path, true);
    let mut query: Vec<(String, String)> = parts
        .url
        .query_pairs()
        .map(|(k, v)| (encode(&k, false), encode(&v, false)))
        .collect();
    query.sort();
    let canonical_query = query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    let canonical_request = format!(
        "{}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{}",
        parts.method, parts.payload_hash
    );
    let date = &parts.amz_date[..8];
    let scope = format!("{date}/{}/{}/aws4_request", parts.region, parts.service);
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        parts.amz_date,
        hex(digest::digest(&digest::SHA256, canonical_request.as_bytes()).as_ref())
    );

    let key = [date, parts.region, parts.service, "aws4_request"]
        .iter()
        .fold(
            format!("AWS4{}", credentials.secret_key).into_bytes(),
            |key, part| mac(&key, part.as_bytes()),
        );
    let signature = hex(&mac(&key, to_sign.as_bytes()));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}", credentials.access_key)
}

fn mac(key: &[u8], data: &[u8]) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data)
        .as_ref()
        .to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// AWS URI encoding: every byte but `A-Z a-z 0-9 - _ . ~` (and `/` in a path) as `%XX`
fn encode(value: &str, path: bool) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b'/' if path => "/".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example credentials of AWS's Signature Version 4 documentation and test suite
    fn example() -> Credentials {
        Credentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
        }
    }

    fn empty_hash() -> String {
        hex(digest::digest(&digest::SHA256, b"").as_ref())
    }

    #[test]
    fn signatures_match_the_aws_test_suite() {
        // get-vanilla of the AWS Signature Version 4 test suite
        let url = reqwest::Url::parse("https://example.amazonaws.com/").unwrap();
        let parts = Parts {
            method: "GET",
            url: &url,
            headers: vec![
                ("host".into(), "example.amazonaws.com".into()),
                ("x-amz-date".into(), "20150830T123600Z".into()),
            ],
            payload_hash: &empty_hash(),
            region: "us-east-1",
            service: "service",
            amz_date: "20150830T123600Z",
        };
        assert_eq!(
            authorization(&parts, &example()),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-date, Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );

        // The IAM ListUsers example of the AWS documentation: a query and a content type
        let url =
            reqwest::Url::parse("https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08")
                .unwrap();
        let parts = Parts {
            method: "GET",
            url: &url,
            headers: vec![
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded; charset=utf-8".into(),
                ),
                ("host".into(), "iam.amazonaws.com".into()),
                ("x-amz-date".into(), "20150830T123600Z".into()),
            ],
            payload_hash: &empty_hash(),
            region: "us-east-1",
            service: "iam",
            amz_date: "20150830T123600Z",
        };
        assert!(authorization(&parts, &example()).ends_with(
            "Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        ));
    }

    #[test]
    fn a_bedrock_request_is_signed_for_its_region_and_service() {
        assert_eq!(
            scope_of("bedrock-runtime.ap-south-1.amazonaws.com"),
            Some(("ap-south-1".into(), "bedrock".into()))
        );
        assert_eq!(
            scope_of("bedrock-mantle.us-east-1.api.aws"),
            Some(("us-east-1".into(), "bedrock-mantle".into()))
        );
        assert_eq!(scope_of("api.openai.com"), None);

        let mut request = reqwest::Client::new()
            .post("https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1/chat/completions")
            .header("content-type", "application/json")
            .body(r#"{"model":"openai.gpt-oss-20b-1:0"}"#)
            .build()
            .unwrap();
        let mut credentials = example();
        credentials.session_token = Some("session-token".into());
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_440_938_160); // 2015-08-30T12:36:00Z
        sign(&mut request, &credentials, at).unwrap();
        let headers = request.headers();
        assert_eq!(headers["x-amz-date"], "20150830T123600Z");
        assert_eq!(headers["x-amz-security-token"], "session-token");
        assert!(headers.get("x-amz-content-sha256").is_none());
        let auth = headers["authorization"].to_str().unwrap();
        // The same request signed by botocore (the AWS SDK for Python) gives this signature
        assert_eq!(auth, "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/bedrock/aws4_request, SignedHeaders=content-type;host;x-amz-date;x-amz-security-token, Signature=5114d1282e88f7bce7af4ccd80fa8a6ae91ea81c3b2179b57cf4502cd3944c3d");
        assert!(sign(
            &mut reqwest::Client::new()
                .get("https://api.openai.com/v1/models")
                .build()
                .unwrap(),
            &credentials,
            at
        )
        .is_err());
    }

    #[test]
    fn a_profile_is_read_from_the_credentials_file() {
        let text = "[default]\naws_access_key_id = AKIA1\naws_secret_access_key = s1\n\n[work]\naws_access_key_id=AKIA2\naws_secret_access_key=s2\naws_session_token = t2\n";
        assert_eq!(
            profile_of(text, "default"),
            Some(Credentials {
                access_key: "AKIA1".into(),
                secret_key: "s1".into(),
                session_token: None
            })
        );
        assert_eq!(
            profile_of(text, "work").unwrap().session_token.as_deref(),
            Some("t2")
        );
        assert_eq!(profile_of(text, "missing"), None);
    }

    #[test]
    fn paths_are_encoded_twice_and_queries_once() {
        assert_eq!(
            encode("/model/anthropic.claude-v2:1/invoke", true),
            "/model/anthropic.claude-v2%3A1/invoke"
        );
        assert_eq!(encode("/a%20b", true), "/a%2520b");
        assert_eq!(encode("a b/c", false), "a%20b%2Fc");
    }
}
