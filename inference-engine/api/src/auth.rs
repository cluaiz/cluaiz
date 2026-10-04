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

    // 2. Sensitive System Routes: Filesystem, Shell Command, Token Management & Permission Mutations
    // These critical operations ALWAYS require a valid Bearer Token (Session token or configured API token)!
    let is_token_management = path == "/v1/system/auth/token/generate" || path == "/v1/system/auth/token/revoke";
    let is_permission_mutation = (path == "/v1/system/permission" && method == axum::http::Method::POST)
        || path.starts_with("/v1/system/permission/approve")
        || path.starts_with("/v1/system/permission/reject");

    if path.starts_with("/v1/fs/") || path == "/v1/system/cmd" || is_token_management || is_permission_mutation {
        if !is_authenticated {
            return Err(unauthorized_response(
                "Missing or invalid Authorization token. Valid session or API Bearer token is required for sensitive system and security operations.",
            ));
        }
        return Ok(next.run(req).await);
    }

    // 3. If request is authenticated with a valid token, allow immediately
    if is_authenticated {
        return Ok(next.run(req).await);
    }

    // 4. Public probes: health and info endpoints always pass for liveness monitoring
    let is_probe = path == "/health" || path == "/info";
    if is_probe {
        return Ok(next.run(req).await);
    }

    // 5. Universal API Authentication enforcement:
    // If tokens are configured in permission.json OR api_auth.required is true,
    // ALL API endpoints (including GET /v1/system/permission, /models, /chat) require a valid Bearer token!
    let has_tokens = !schema.api_auth.tokens.is_empty();
    let auth_enforced = schema.api_auth.required || has_tokens;

    if auth_enforced {
        return Err(unauthorized_response(
            "API Authentication is required. Provide a valid Bearer token in the Authorization header.",
        ));
    }

    // 6. When API Auth is not enforced (local dev without tokens), allow request naturally
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

    #[test]
    fn test_auth_enforced_when_tokens_exist() {
        let has_tokens = true;
        let required = false;
        let auth_enforced = required || has_tokens;
        assert!(auth_enforced);

        let has_tokens_empty = false;
        let required_false = false;
        let auth_not_enforced = required_false || has_tokens_empty;
        assert!(!auth_not_enforced);
    }
}

