
/// Nebula Registry Client
/// Handles communication with GitHub and Cloudflare for manifests and stats.
pub struct RegistryClient {
    base_url: String, // e.g., "https://raw.githubusercontent.com/cluaiz/nebula-registry/main"
}

impl RegistryClient {
    pub fn new(base_url: String) -> Self {
        Self { 
            base_url: base_url.trim_end_matches('/').to_string() 
        }
    }

    /// Fetches the master_index.json from the remote registry
    pub async fn fetch_index(&self) -> Result<String, String> {
        let url = format!("{}/index.json", self.base_url);
        self.get_request(&url).await
    }

    /// Fetches a specific manifest.json for a model variant from the library
    /// Pattern: library/[family]/v-[version]/[id].json
    pub async fn fetch_manifest(&self, family: &str, version: &str, id: &str) -> Result<String, String> {
        let url = format!(
            "{}/library/{}/v-{}/{}.json",
            self.base_url, family, version, id
        );
        self.get_request(&url).await
    }

    /// Fetches the README.md for a model version
    pub async fn fetch_readme(&self, family: &str, version: &str) -> Result<String, String> {
        let url = format!(
            "{}/library/{}/v-{}/README.md",
            self.base_url, family, version
        );
        self.get_request(&url).await
    }

    /// Internal helper for GET requests with industrial timeout
    async fn get_request(&self, url: &str) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Cluaiz Client Error: {}", e))?;

        let response = client.get(url)
            .header("User-Agent", "Cluaiz-Cluaiz-Engine")
            .send()
            .await
            .map_err(|e| format!("Network Handshake Failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Registry Error ({}): Model not found at {}", response.status(), url));
        }

        response.text().await.map_err(|e| format!("Failed to read registry response: {}", e))
    }
}
