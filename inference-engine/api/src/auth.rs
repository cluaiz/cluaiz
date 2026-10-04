use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use engines::neural_foundry::security::permission_schema::PermissionSchema;
use crate::state::AppState;

/// Constant-time string comparison to prevent side-channel timing attacks
pub fn constant_time_compare(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Generates a cryptographically strong session token on engine boot and writes it to ~/.cluaiz/session.token
pub fn initialize_session_token() -> String {
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    if let Some(home) = dirs::home_dir() {
        let token_dir = home.join(".cluaiz");
        let _ = std::fs::create_dir_all(&token_dir);
        let token_file = token_dir.join("session.token");
        if let Ok(_) = std::fs::write(&token_file, &token) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600));
            }
            tracing::info!("🔒 [Auth] Initialized local session secret at {}", token_file.display());
        }
    }
    token
}

/// Extracts Bearer token from the Authorization header if present
pub fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .map(|val| {
            let val_trimmed = val.trim();
            if val_trimmed.starts_with("Bearer ") {
                val_trimmed.trim_start_matches("Bearer ").trim().to_string()
            } else {
                val_trimmed.to_string()
            }
        })
}

/// Pure Bearer Token Authentication Middleware (Market Standard: OpenAI / vLLM compatible)
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    // 0. Preflight OPTIONS requests always pass directly to CORS layer
    if req.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // 1. Extract Bearer Token from Authorization header
    let bearer_token = extract_bearer_token(&req);

    let schema = PermissionSchema::load();
    let is_authenticated = if let Some(ref token) = bearer_token {
        let is_session_valid = constant_time_compare(token, &state.session_token);
        let is_configured_valid = schema.api_auth.tokens.iter().any(|t| constant_time_compare(token, t));
        is_session_valid || is_configured_valid
    } else {
        false
    };

    // 2. Sensitive System Routes: Filesystem (/v1/fs/*) and Terminal (/v1/system/cmd)
    // Filesystem and shell commands ALWAYS require a valid Bearer Token!
    if path.starts_with("/v1/fs/") || path == "/v1/system/cmd" {
        if !is_authenticated {
            return Err(unauthorized_response(
                "Missing or invalid Authorization token. Session or API Bearer token is required for sensitive system operations.",
            ));
        }
        return Ok(next.run(req).await);
    }

    // Permission Mutations: Require token if API authentication is enabled
    let is_permission_mutation = (path == "/v1/system/permission" && method == axum::http::Method::POST)
        || path.starts_with("/v1/system/permission/approve")
        || path.starts_with("/v1/system/permission/reject");

    if is_permission_mutation {
        if schema.api_auth.required && !schema.api_auth.tokens.is_empty() && !is_authenticated {
            return Err(unauthorized_response(
                "Missing or invalid Authorization token. Bearer token is required when API authentication is enabled.",
            ));
        }
        return Ok(next.run(req).await);
    }

    // 3. If request is authenticated with a valid token, allow immediately
    if is_authenticated {
        return Ok(next.run(req).await);
    }

    // 4. Public endpoints: health/info probes, permission schema for UI initialization, and DevHub static UI files
    let is_api_route = path.starts_with("/v1/") 
        || path.starts_with("/api/") 
        || path.starts_with("/models/") 
        || path.starts_with("/engine/") 
        || path.starts_with("/hardware");

    let is_public = path == "/health" 
        || path == "/info" 
        || (path == "/v1/system/permission" && method == axum::http::Method::GET);

    if !is_api_route || is_public {
        return Ok(next.run(req).await);
    }

    // 5. Global API Authentication enforcement
    // When "Require API Authentication" is enabled and tokens are configured, reject all unauthenticated API calls
    if schema.api_auth.required && !schema.api_auth.tokens.is_empty() {
        return Err(unauthorized_response(
            "API Authentication is required. Provide a valid Bearer token in the Authorization header.",
        ));
    }

    // 6. When API Auth is not required (Local Dev Mode), allow request naturally
    Ok(next.run(req).await)
}

fn unauthorized_response(msg: &str) -> Response {
    let body = Json(json!({
        "error": "Unauthorized",
        "message": msg
    }));
    (StatusCode::UNAUTHORIZED, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("secret_token_123", "secret_token_123"));
        assert!(!constant_time_compare("secret_token_123", "secret_token_456"));
        assert!(!constant_time_compare("secret", "secret_longer"));
    }

    #[test]
    fn test_token_authentication_flow() {
        let session_token = "session_test_secret_12345";
        let api_token = "sk-cluaiz-e46c11e877b20afb49c4df0fcfa44351";
        let configured_tokens = vec![api_token.to_string()];

        // 1. Session token matches
        assert!(constant_time_compare("session_test_secret_12345", session_token));

        // 2. Configured API key matches
        assert!(configured_tokens.iter().any(|t| constant_time_compare(api_token, t)));

        // 3. Invalid token fails
        let invalid = "sk-cluaiz-wrongkey";
        assert!(!constant_time_compare(invalid, session_token));
        assert!(!configured_tokens.iter().any(|t| constant_time_compare(invalid, t)));
    }

    #[test]
    fn test_permission_mutation_requires_auth() {
        let path = "/v1/system/permission";
        let method_post = axum::http::Method::POST;
        let is_mutation = (path == "/v1/system/permission" && method_post == axum::http::Method::POST)
            || path.starts_with("/v1/system/permission/approve")
            || path.starts_with("/v1/system/permission/reject");
        assert!(is_mutation);

        let method_get = axum::http::Method::GET;
        let is_get_mutation = (path == "/v1/system/permission" && method_get == axum::http::Method::POST)
            || path.starts_with("/v1/system/permission/approve")
            || path.starts_with("/v1/system/permission/reject");
        assert!(!is_get_mutation);
    }
}
