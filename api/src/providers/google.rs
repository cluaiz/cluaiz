//! A Google Cloud access token for Vertex AI (`auth: "google"`), as Google's client libraries get
//! one (Application Default Credentials): the JSON file named in `GOOGLE_APPLICATION_CREDENTIALS`,
//! else the file `gcloud auth application-default login` writes, else `gcloud auth
//! print-access-token`. A service account file is signed into a token request (RS256 JWT); a user
//! login is refreshed with its refresh token. A token lives an hour; it is kept and renewed five
//! minutes before it ends. No token is ever logged.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{json, Value};

const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// A token is renewed this long before it ends
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
    let (token, lasts) = fetch().await?;
    if let Ok(mut cached) = CACHED.lock() {
        *cached = Some((token.clone(), Instant::now() + lasts));
    }
    Ok(token)
}

/// Where Application Default Credentials are: the variable, else gcloud's file
fn credentials_file() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(path);
    }
    let folder = if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("gcloud"))
    } else {
        std::env::var_os("CLOUDSDK_CONFIG")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("gcloud"))
            })
    };
    folder
        .map(|f| f.join("application_default_credentials.json"))
        .filter(|p| p.is_file())
}

async fn fetch() -> Result<(String, Duration), String> {
    if let Some(path) = credentials_file() {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file: Value = serde_json::from_str(&text)
            .map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
        let form = match file["type"].as_str() {
            Some("service_account") => {
                let token_url = file["token_uri"].as_str().unwrap_or(TOKEN_URL);
                let assertion = signed_assertion(&file, token_url, now_seconds())?;
                vec![("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer".to_string()), ("assertion", assertion)]
            }
            Some("authorized_user") => vec![
                ("grant_type", "refresh_token".to_string()),
                ("client_id", file["client_id"].as_str().unwrap_or_default().to_string()),
                ("client_secret", file["client_secret"].as_str().unwrap_or_default().to_string()),
                ("refresh_token", file["refresh_token"].as_str().unwrap_or_default().to_string()),
            ],
            other => return Err(format!("{}: credentials of type {other:?} are not read; use a service account file or `gcloud auth application-default login`", path.display())),
        };
        let token_url = file["token_uri"]
            .as_str()
            .filter(|_| file["type"] == "service_account")
            .unwrap_or(TOKEN_URL)
            .to_string();
        return exchange(&token_url, &form).await;
    }
    from_gcloud().await
}

/// Posts the token request; the token and how long it lasts
async fn exchange(url: &str, form: &[(&str, String)]) -> Result<(String, Duration), String> {
    let body: String = form
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let response = super::http::client()
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .timeout(Duration::from_secs(30))
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Google sign-in did not answer: {e}"))?;
    let status = response.status().as_u16();
    let answer: Value = response.json().await.unwrap_or_default();
    match answer["access_token"].as_str() {
        Some(token) if (200..300).contains(&status) => Ok((token.to_string(), Duration::from_secs(answer["expires_in"].as_u64().unwrap_or(3600)))),
        _ => Err(format!(
            "Google refused the sign-in ({status}): {}; run `gcloud auth application-default login`, or set GOOGLE_APPLICATION_CREDENTIALS to a service account file",
            answer["error_description"].as_str().or_else(|| answer["error"].as_str()).unwrap_or("no reason given")
        )),
    }
}

/// `gcloud auth print-access-token`, when there is no credentials file
async fn from_gcloud() -> Result<(String, Duration), String> {
    let program = if cfg!(windows) {
        "gcloud.cmd"
    } else {
        "gcloud"
    };
    let output = tokio::process::Command::new(program)
        .args(["auth", "print-access-token"])
        .output()
        .await
        .map_err(|_| "no Google Cloud sign-in: run `gcloud auth application-default login`, or set GOOGLE_APPLICATION_CREDENTIALS to a service account file".to_string())?;
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || token.is_empty() {
        return Err("gcloud gave no access token: run `gcloud auth login`, or `gcloud auth application-default login`".into());
    }
    // gcloud's tokens last an hour; renewed before that
    Ok((token, Duration::from_secs(3000)))
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A service account's signed token request (RS256 JWT) for the cloud-platform scope
fn signed_assertion(file: &Value, audience: &str, now: u64) -> Result<String, String> {
    let email = file["client_email"]
        .as_str()
        .ok_or("the service account file has no client_email")?;
    let pem = file["private_key"]
        .as_str()
        .ok_or("the service account file has no private_key")?;
    let header = json!({ "alg": "RS256", "typ": "JWT" });
    let claims =
        json!({ "iss": email, "scope": SCOPE, "aud": audience, "iat": now, "exp": now + 3600 });
    let signing_input = format!(
        "{}.{}",
        b64url(header.to_string().as_bytes()),
        b64url(claims.to_string().as_bytes())
    );
    let der = pem_body(pem).ok_or("the service account's private_key is not a PEM key")?;
    let key = ring::signature::RsaKeyPair::from_pkcs8(&der)
        .map_err(|e| format!("the service account's private_key cannot be read: {e}"))?;
    let mut signature = vec![0u8; key.public().modulus_len()];
    key.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        signing_input.as_bytes(),
        &mut signature,
    )
    .map_err(|_| "the service account's key could not sign".to_string())?;
    Ok(format!("{signing_input}.{}", b64url(&signature)))
}

/// The bytes of a PEM block
fn pem_body(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----") && !l.is_empty())
        .collect();
    base64::engine::general_purpose::STANDARD.decode(body).ok()
}

fn urlencode(value: &str) -> String {
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

    /// A throwaway 2048-bit test key (PKCS#8), made for this test only
    const TEST_KEY: &str = include_str!("tests/fixtures/test_service_account_key.pem");

    #[test]
    fn a_service_account_signs_a_token_request_that_verifies() {
        let file = json!({ "type": "service_account", "client_email": "bot@project.iam.gserviceaccount.com", "private_key": TEST_KEY, "token_uri": TOKEN_URL });
        let jwt = signed_assertion(&file, TOKEN_URL, 1_700_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let decode = |p: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(p)
                .unwrap()
        };
        let header: Value = serde_json::from_slice(&decode(parts[0])).unwrap();
        let claims: Value = serde_json::from_slice(&decode(parts[1])).unwrap();
        assert_eq!(header["alg"], "RS256");
        assert_eq!(claims["iss"], "bot@project.iam.gserviceaccount.com");
        assert_eq!(claims["scope"], SCOPE);
        assert_eq!(claims["aud"], TOKEN_URL);
        assert_eq!(claims["exp"].as_u64(), Some(1_700_003_600));
        // The signature verifies with the key's public half
        let key = ring::signature::RsaKeyPair::from_pkcs8(&pem_body(TEST_KEY).unwrap()).unwrap();
        use ring::signature::KeyPair;
        let public = ring::signature::UnparsedPublicKey::new(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            key.public_key().as_ref(),
        );
        assert!(public
            .verify(
                format!("{}.{}", parts[0], parts[1]).as_bytes(),
                &decode(parts[2])
            )
            .is_ok());
        // A file without a key says so
        assert!(
            signed_assertion(&json!({ "client_email": "x" }), TOKEN_URL, 0)
                .unwrap_err()
                .contains("private_key")
        );
    }

    #[test]
    fn form_values_are_encoded() {
        assert_eq!(urlencode("a b/c+d=e"), "a%20b%2Fc%2Bd%3De");
        assert_eq!(urlencode("urn:ietf"), "urn%3Aietf");
    }
}
