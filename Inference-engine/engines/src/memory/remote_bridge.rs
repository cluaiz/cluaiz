//! 🌐 Remote Bridge: Network-based client connection to central cluaizd daemon.
//! Leverages HTTP transport to synchronize neuron states across clustered nodes.

use super::storage_bridge::CognitiveStorageBridge;
use std::time::Duration;
use reqwest::Client;
use serde_json::json;
use crate::neural_foundry::security::permission_schema::PermissionSchema;
use sha2::Digest;

pub struct RemoteBridge {
    client: Client,
    base_url: String,
}

impl RemoteBridge {
    pub fn new(addr: &str) -> Self {
        // Prepend http:// if not present
        let mut base_url = addr.trim().to_string();
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            base_url = format!("http://{}", base_url);
        }

        // Initialize connection pooling with a strict timeout of 1.5s
        let client = Client::builder()
            .timeout(Duration::from_millis(1500))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_default();

        RemoteBridge { client, base_url }
    }

    async fn execute_with_retry<F, Fut, T>(&self, mut operation: F) -> Option<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        let mut retries = 3;
        while retries > 0 {
            match operation().await {
                Ok(res) => return Some(res),
                Err(e) => {
                    tracing::warn!("Remote database query failed ({} retries left): {}", retries - 1, e);
                    retries -= 1;
                    if retries > 0 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
        None
    }
}

impl CognitiveStorageBridge for RemoteBridge {
    fn inject_context(&self, memory_key: &str) -> Option<Vec<u8>> {
        // 1. Generate 16-byte UUID from the memory_key string using stable Sha256 hashing
        let mut hasher = sha2::Sha256::new();
        hasher.update(memory_key.as_bytes());
        let hash_result = hasher.finalize();
        let mut id_array = [0u8; 16];
        id_array.copy_from_slice(&hash_result[..16]);
        let uuid = uuid::Uuid::from_bytes(id_array);
        let uuid_str = uuid.to_string();

        let url = format!("{}/neuron/{}", self.base_url, uuid_str);
        
        let client = self.client.clone();
        
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                // 2. Perform GET query with retry strategy using execute_with_retry helper
                let response_body = self.execute_with_retry(|| {
                    let client = client.clone();
                    let url = url.clone();
                    async move {
                        let res = client.get(&url)
                            .header("x-tenant-id", "default_sandbox")
                            .send()
                            .await?;
                        if !res.status().is_success() {
                            anyhow::bail!("HTTP Error: {}", res.status());
                        }
                        let text = res.text().await?;
                        Ok(text)
                    }
                }).await?;
                
                // 3. Parse and extract payload bytes
                let json_val: serde_json::Value = serde_json::from_str(&response_body).ok()?;
                if let Some(payload_val) = json_val.get("raw_payload") {
                    if let Ok(bytes) = serde_json::from_value::<Vec<u8>>(payload_val.clone()) {
                        tracing::info!("🧠 Successfully injected Remote Context: {} bytes", bytes.len());
                        return Some(bytes);
                    }
                }
                
                None
            })
        })
    }

    fn save_context(&self, memory_id: &str, payload: &str, vector: &[f32]) -> Result<(), String> {
        let url = format!("{}/neuron", self.base_url);
        
        // Pull the active embedding model ID directly from the active PermissionSchema
        let schema = PermissionSchema::load();
        let creator_hash = schema.get_active_embedding_model().unwrap_or_else(|| {
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
        });
        
        let body = json!({
            "raw_payload": payload,
            "vector_data": vector,
            "model_creator_hash": creator_hash,
            "payload_type": "text",
            "dna": null,
            "adjacency": null
        });

        let client = self.client.clone();
        
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let res = self.execute_with_retry(|| {
                    let client = client.clone();
                    let url = url.clone();
                    let body = body.clone();
                    async move {
                        let res = client.post(&url)
                            .header("x-tenant-id", "default_sandbox")
                            .json(&body)
                            .send()
                            .await?;
                        if !res.status().is_success() {
                            anyhow::bail!("HTTP Error: {}", res.status());
                        }
                        Ok(())
                    }
                }).await;
                if res.is_some() {
                    Ok(())
                } else {
                    Err("Failed to execute remote network database write after retries".to_string())
                }
            })
        })
    }
}

