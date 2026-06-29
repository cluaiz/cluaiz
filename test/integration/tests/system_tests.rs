use reqwest::Client;
use serde_json::Value;

const BASE_URL: &str = "http://127.0.0.1:8000";

#[tokio::test]
async fn test_root_endpoint() {
    let client = Client::new();
    let res = client.get(BASE_URL)
        .send()
        .await
        .expect("Failed to connect to engine. Is it running on port 8000?");
    
    assert!(res.status().is_success());
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["engine"], "cluaiz Inference Engine");
}

#[tokio::test]
async fn test_health_check() {
    let client = Client::new();
    let res = client.get(&format!("{}/health", BASE_URL))
        .send()
        .await
        .expect("Failed to execute request");
    
    assert!(res.status().is_success());
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "alive");
}

#[tokio::test]
async fn test_system_info() {
    let client = Client::new();
    let res = client.get(&format!("{}/info", BASE_URL))
        .send()
        .await
        .expect("Failed to execute request");
    
    assert!(res.status().is_success());
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["engine"], "cluaiz");
}

#[tokio::test]
async fn test_brain_toggle() {
    let client = Client::new();
    // Test pure brain mode payload
    let payload = serde_json::json!({
        "state": "only_brain"
    });

    let res = client.post(&format!("{}/v1/system/brain", BASE_URL))
        .json(&payload)
        .send()
        .await
        .expect("Failed to execute request");
    
    assert!(res.status().is_success());
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "success");
}
