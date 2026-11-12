//! A Microsoft Entra ID access token for Azure OpenAI and Microsoft Foundry (`auth: "azure"`), as
//! Azure's client libraries get one: a service principal in `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`
//! and `AZURE_CLIENT_SECRET`, else the Azure CLI's sign-in (`az account get-access-token`). The
//! token is for `https://cognitiveservices.azure.com` (`AZURE_TOKEN_SCOPE` sets another, such as
//! `https://ai.azure.com/.default`); it is kept and renewed five minutes before it ends, and sent
//! as `Authorization: Bearer`. No token is ever logged.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

const SCOPE: &str = "https://cognitiveservices.azure.com/.default";
const RENEW_BEFORE: Duration = Duration::from_secs(300);

static CACHED: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// A current access token, from the cache or a new one
pub(super) async fn access_token() -> Result<String, String> {
    if let Ok(cached) = CACHED.lock() {
        if let Some((token, _)) = cached
            .as_ref()
            .filter(|(_, until)| Instant::now() + RENEW_BEFORE < *until)
        {
            return Ok(token.clone());
        }
    }
    let (token, lasts) = match service_principal() {
        Some((tenant, client, secret)) => from_secret(&tenant, &client, &secret).await?,
        None => from_cli().await?,
    };
    if let Ok(mut cached) = CACHED.lock() {
        *cached = Some((token.clone(), Instant::now() + lasts));
    }
    Ok(token)
}

fn scope() -> String {
    std::env::var("AZURE_TOKEN_SCOPE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SCOPE.to_string())
}

fn service_principal() -> Option<(String, String, String)> {
    let value = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Some((
        value("AZURE_TENANT_ID")?,
        value("AZURE_CLIENT_ID")?,
        value("AZURE_CLIENT_SECRET")?,
    ))
}

/// The token request of a service principal (OAuth 2.0 client credentials)
fn secret_form(client: &str, secret: &str, scope: &str) -> String {
    [
        ("grant_type", "client_credentials"),
        ("client_id", client),
        ("client_secret", secret),
        ("scope", scope),
    ]
    .iter()
    .map(|(k, v)| format!("{k}={}", form_encode(v)))
    .collect::<Vec<_>>()
    .join("&")
}

async fn from_secret(
    tenant: &str,
    client: &str,
    secret: &str,
) -> Result<(String, Duration), String> {
    let authority = std::env::var("AZURE_AUTHORITY_HOST")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| "https://login.microsoftonline.com".into());
    let url = format!(
        "{}/{}/oauth2/v2.0/token",
        authority.trim_end_matches('/'),
        form_encode(tenant)
    );
    let response = super::http::client()
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .timeout(Duration::from_secs(30))
        .body(secret_form(client, secret, &scope()))
        .send()
        .await
        .map_err(|e| format!("Microsoft Entra sign-in did not answer: {e}"))?;
    let status = response.status().as_u16();
    let answer: Value = response.json().await.unwrap_or_default();
    match answer["access_token"].as_str() {
        Some(token) if (200..300).contains(&status) => Ok((token.to_string(), Duration::from_secs(answer["expires_in"].as_u64().unwrap_or(3600)))),
        _ => Err(format!(
            "Microsoft Entra refused the sign-in ({status}): {}; check AZURE_TENANT_ID, AZURE_CLIENT_ID and AZURE_CLIENT_SECRET",
            answer["error_description"].as_str().or_else(|| answer["error"].as_str()).unwrap_or("no reason given").lines().next().unwrap_or_default()
        )),
    }
}

/// `az account get-access-token`, when there is no service principal
async fn from_cli() -> Result<(String, Duration), String> {
    let program = if cfg!(windows) { "az.cmd" } else { "az" };
    let resource = scope().trim_end_matches("/.default").to_string();
    let output = tokio::process::Command::new(program)
        .args(["account", "get-access-token", "--resource", &resource, "--output", "json"])
        .output()
        .await
        .map_err(|_| "no Microsoft Entra sign-in: run `az login`, set AZURE_TENANT_ID, AZURE_CLIENT_ID and AZURE_CLIENT_SECRET, or use the resource's API key".to_string())?;
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    cli_token(&answer, chrono::Utc::now().timestamp())
        .ok_or_else(|| "the Azure CLI gave no access token: run `az login`".to_string())
}

/// The token in `az account get-access-token` output, and how long it lasts from `now`
fn cli_token(answer: &Value, now: i64) -> Option<(String, Duration)> {
    let token = answer["accessToken"]
        .as_str()
        .filter(|t| !t.is_empty())?
        .to_string();
    let ends = answer["expires_on"]
        .as_i64()
        .or_else(|| answer["expires_on"].as_str().and_then(|s| s.parse().ok()));
    let lasts = ends
        .map(|end| Duration::from_secs((end - now).max(0) as u64))
        .unwrap_or(Duration::from_secs(3000));
    Some((token, lasts))
}

fn form_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_service_principal_asks_for_the_cognitive_services_scope() {
        let form = secret_form("app-id", "s3cr=t&x", SCOPE);
        assert_eq!(form, "grant_type=client_credentials&client_id=app-id&client_secret=s3cr%3Dt%26x&scope=https%3A%2F%2Fcognitiveservices.azure.com%2F.default");
    }

    #[test]
    fn the_cli_token_and_its_end_are_read() {
        let answer = json!({ "accessToken": "eyJ0", "expiresOn": "2026-11-11 12:00:00.000000", "expires_on": 1_800_003_600, "tokenType": "Bearer" });
        assert_eq!(
            cli_token(&answer, 1_800_000_000),
            Some(("eyJ0".to_string(), Duration::from_secs(3600)))
        );
        assert_eq!(
            cli_token(
                &json!({ "accessToken": "t", "expires_on": "1800000060" }),
                1_800_000_000
            )
            .unwrap()
            .1,
            Duration::from_secs(60)
        );
        assert_eq!(cli_token(&json!({}), 0), None);
    }
}
