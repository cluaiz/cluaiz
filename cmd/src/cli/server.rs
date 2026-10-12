//! Requests to a running `cluaiz serve`: same port rule as the server, authenticated with the session
//! token the server writes for local clients.

use cluaiz_security::PermissionSchema;

pub fn base_url() -> String {
    let perms = PermissionSchema::load();
    format!("{}://localhost:{}", perms.connection_protocol, cluaiz_api::api_port(&perms))
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
