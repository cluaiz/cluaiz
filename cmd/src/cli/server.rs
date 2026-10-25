//! Requests to a running `cluaiz serve`: same port rule as the server, authenticated with the session
//! token the server writes for local clients.

use cluaiz_security::PermissionSchema;

pub fn base_url() -> String {
    let perms = PermissionSchema::load();
    format!(
        "{}://localhost:{}",
        perms.connection_protocol,
        cluaiz_api::api_port(&perms)
    )
}

pub fn request(method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let builder = client.request(method, format!("{}{}", base_url(), path));
    match cluaiz_api::auth::session_token_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(token) => builder.bearer_auth(token.trim()),
        None => builder,
    }
}

/// One authenticated CEP request for CLI callers. RPC errors are real command failures.
pub async fn cep(method: &str, params: serde_json::Value) -> color_eyre::Result<serde_json::Value> {
    use color_eyre::eyre::eyre;
    let response = request(reqwest::Method::POST, "/v1/cep")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .map_err(|error| eyre!("{method} at {} failed: {error}", base_url()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!("Engine returned HTTP {status} for {method}"));
    }
    let reply: serde_json::Value = response.json().await?;
    if reply["jsonrpc"] != "2.0" || reply["id"] != 1 {
        return Err(eyre!("Invalid CEP response for {method}"));
    }
    if let Some(error) = reply.get("error") {
        return Err(eyre!(
            "{method}: {} (code {})",
            error["message"].as_str().unwrap_or("CEP error"),
            error["code"]
        ));
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| eyre!("Missing CEP result for {method}"))
}
