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
