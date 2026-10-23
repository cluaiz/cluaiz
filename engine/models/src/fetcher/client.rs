//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Registry Client for Remote CDN & Index
//! ═══════════════════════════════════════════════════════════════════════

use reqwest::Client;

pub struct RegistryClient {
    base_url: String,
}

impl RegistryClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Fetches the master index.json from remote registry
    pub async fn fetch_index(&self) -> Result<String, String> {
        let url = format!("{}/index.json", self.base_url);
        self.get_request(&url).await
    }

    /// Fetches a specific manifest JSON for a model variant
    pub async fn fetch_manifest(
        &self,
        family: &str,
        version: &str,
        id: &str,
    ) -> Result<String, String> {
        let url = format!(
            "{}/library/{}/v-{}/{}.json",
            self.base_url, family, version, id
        );
        self.get_request(&url).await
    }

    async fn get_request(&self, url: &str) -> Result<String, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Client Error: {}", e))?;

        let response = client
            .get(url)
            .header("User-Agent", "Cluaiz-Engine/1.0")
            .send()
            .await
            .map_err(|e| format!("Network Handshake Failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Registry Error ({}): Not found at {}",
                response.status(),
                url
            ));
        }

        response
            .text()
            .await
            .map_err(|e| format!("Failed to read registry response: {}", e))
    }
}

const RESOLVER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
const MAX_RESOLVER_BYTES: usize = 64 * 1024;
const MAX_MODEL_ID_BYTES: usize = 256;
const HF_PULL_HINT: &str = "Retry later or use cluaiz pull <owner>/<repo>[:quant] with the full Hugging Face repository ID.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryResolveError {
    Transport(String),
    RateLimited(Option<u64>),
    ClientRejected(u16),
    Unavailable(u16),
    InvalidResponse,
    InvalidInput,
    NotFound(String),
}

impl std::fmt::Display for RegistryResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(detail) => write!(f, "Cluaiz registry connection failed ({detail}). {HF_PULL_HINT}"),
            Self::RateLimited(Some(seconds)) => write!(f, "Cluaiz registry request limit reached. Wait {seconds} seconds. {HF_PULL_HINT}"),
            Self::RateLimited(None) => write!(f, "Cluaiz registry request limit reached. {HF_PULL_HINT}"),
            Self::ClientRejected(status) => write!(f, "Cluaiz registry rejected this client or gateway (HTTP {status}). Update the Engine or use a full Hugging Face repository ID; no login is required."),
            Self::Unavailable(status) => write!(f, "Cluaiz registry unavailable (HTTP {status}). {HF_PULL_HINT}"),
            Self::InvalidResponse => write!(f, "Cluaiz registry returned an invalid response. {HF_PULL_HINT}"),
            Self::InvalidInput => write!(f, "Invalid model ID. Use a catalogue ID or a full Hugging Face repository ID."),
            Self::NotFound(id) => write!(f, "No matching Cluaiz model ID '{id}'. {HF_PULL_HINT}"),
        }
    }
}

impl std::error::Error for RegistryResolveError {}

/// Normalize public HF coordinates without allowing paths, query strings or another host.
pub fn parse_hf_repository(value: &str) -> Option<String> {
    let repo = value
        .trim()
        .strip_prefix("hf://")
        .or_else(|| value.trim().strip_prefix("https://huggingface.co/"))
        .unwrap_or(value.trim())
        .trim_end_matches('/');
    let segments: Vec<_> = repo.split('/').collect();
    if repo.len() > MAX_MODEL_ID_BYTES
        || segments.len() != 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || *segment == "."
                || *segment == ".."
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return None;
    }
    Some(repo.to_string())
}

/// Compatibility entry point; all registry requests have a single HTTP owner.
pub async fn resolve_model_repo(model_id: &str) -> Result<Option<String>, RegistryResolveError> {
    resolve_model_repo_with_fallback(model_id, None).await
}

/// Resolve original and optional tagged-base IDs in one HTTP request. No telemetry is sent.
pub async fn resolve_model_repo_with_fallback(
    model_id: &str,
    fallback: Option<&str>,
) -> Result<Option<String>, RegistryResolveError> {
    let clean_id = model_id.trim();
    if clean_id.contains('/') {
        return parse_hf_repository(clean_id)
            .map(Some)
            .ok_or(RegistryResolveError::InvalidInput);
    }
    if clean_id.is_empty()
        || clean_id.len() > MAX_MODEL_ID_BYTES
        || !clean_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:+-".contains(&byte))
    {
        return Err(RegistryResolveError::InvalidInput);
    }
    let fallback = fallback.filter(|base| *base != clean_id);
    if fallback.is_some_and(|base| {
        base.is_empty()
            || base.len() > MAX_MODEL_ID_BYTES
            || !clean_id.starts_with(&format!("{base}:"))
    }) {
        return Err(RegistryResolveError::InvalidInput);
    }
    let endpoint = std::env::var("CLUAIZ_RESOLVER_ENDPOINT")
        .unwrap_or_else(|_| "https://cluaiz.com/api/models/resolve".to_string());
    let client = Client::builder()
        .timeout(RESOLVER_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| RegistryResolveError::Transport(error.without_url().to_string()))?;
    let mut query = vec![("id", clean_id)];
    if let Some(base) = fallback {
        query.push(("fallback_id", base));
    }
    let mut response = client
        .get(&endpoint)
        .query(&query)
        .header(
            "User-Agent",
            concat!("Cluaiz-Inference-Engine/", env!("CARGO_PKG_VERSION")),
        )
        .header("X-Cluaiz-Client", "native-cli")
        .send()
        .await
        .map_err(|error| RegistryResolveError::Transport(error.without_url().to_string()))?;
    let status = response.status().as_u16();
    if status == 429 {
        let retry = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        return Err(RegistryResolveError::RateLimited(retry));
    }
    if status == 401 || status == 403 {
        return Err(RegistryResolveError::ClientRejected(status));
    }
    if status == 400 || status == 413 {
        return Err(RegistryResolveError::InvalidInput);
    }
    if !response.status().is_success() {
        return Err(RegistryResolveError::Unavailable(status));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESOLVER_BYTES as u64)
    {
        return Err(RegistryResolveError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| RegistryResolveError::Transport(error.without_url().to_string()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESOLVER_BYTES {
            return Err(RegistryResolveError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| RegistryResolveError::InvalidResponse)?;
    match json.get("found").and_then(|value| value.as_bool()) {
        Some(false) => Ok(None),
        Some(true) => {
            let repo = json
                .get("hf_repo")
                .and_then(|value| value.as_str())
                .and_then(parse_hf_repository)
                .ok_or(RegistryResolveError::InvalidResponse)?;
            Ok(Some(repo))
        }
        None => Err(RegistryResolveError::InvalidResponse),
    }
}
