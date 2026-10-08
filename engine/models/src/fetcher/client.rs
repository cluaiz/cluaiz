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
    pub async fn fetch_manifest(&self, family: &str, version: &str, id: &str) -> Result<String, String> {
        let url = format!("{}/library/{}/v-{}/{}.json", self.base_url, family, version, id);
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
            return Err(format!("Registry Error ({}): Not found at {}", response.status(), url));
        }

        response.text().await.map_err(|e| format!("Failed to read registry response: {}", e))
    }
}

/// Dispatches an anonymous, non-blocking telemetry event to record model pull execution.
/// 100% fail-safe: 0ms blocking, zero noise, and respects DO_NOT_TRACK / CLUAIZ_TELEMETRY=0.
pub fn dispatch_model_telemetry(target_id: &str) {
    if std::env::var("DO_NOT_TRACK").map(|v| v == "1").unwrap_or(false)
        || std::env::var("CLUAIZ_TELEMETRY").map(|v| v == "0").unwrap_or(false)
    {
        return;
    }

    let id = target_id.to_string();
    if id.trim().is_empty() {
        return;
    }

    tokio::spawn(async move {
        let endpoint = std::env::var("CLUAIZ_TELEMETRY_ENDPOINT")
            .unwrap_or_else(|_| "https://cluaiz.com/api/models/download".to_string());

        let client = match Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        let payload = serde_json::json!({
            "modelId": id
        });

        let _ = client
            .post(&endpoint)
            .header("User-Agent", "Cluaiz-Inference-Engine/1.0")
            .header("X-Cluaiz-Client", "native-cli")
            .json(&payload)
            .send()
            .await;
    });
}

/// Dynamically resolves a short Model ID (e.g. `qwen:0.6b`, `whisper-base`)
/// to its upstream Hugging Face repository from the Cluaiz Neural Registry API.
pub async fn resolve_model_repo(model_id: &str) -> Result<Option<String>, String> {
    let clean_id = model_id.trim();
    if clean_id.is_empty() {
        return Ok(None);
    }

    let endpoint = std::env::var("CLUAIZ_RESOLVER_ENDPOINT")
        .unwrap_or_else(|_| "https://cluaiz.com/api/models/resolve".to_string());

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| format!("Resolver Client Error: {}", e))?;

    let response = client
        .get(&endpoint)
        .query(&[("id", clean_id)])
        .header("User-Agent", "Cluaiz-Inference-Engine/1.0")
        .send()
        .await
        .map_err(|e| format!("Resolver Handshake Failed: {}", e))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid JSON from resolver: {}", e))?;

    if json.get("found").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(hf_repo) = json.get("hf_repo").and_then(|v| v.as_str()) {
            if !hf_repo.trim().is_empty() {
                return Ok(Some(hf_repo.trim().to_string()));
            }
        }
    }

    Ok(None)
}
