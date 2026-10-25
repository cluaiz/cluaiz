//! 🔒 Bearer Token Authentication & Security Middleware (`api/src/auth.rs`)
//! Validates session tokens and API tokens across endpoints, keeping /health and /info public.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

/// Constant-time string comparison to prevent timing attacks
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

/// Session token in the selected Cluaiz home; local clients use the same path.
/// EnvironmentManager preserves the default ~/.cluaiz path and portable/home overrides.
pub fn session_token_path() -> Option<std::path::PathBuf> {
    Some(
        cluaiz_core::EnvironmentManager::current()
            .global_dir
            .join("session.token"),
    )
}

/// Generates a session token on engine boot and writes it into the selected home.
pub fn initialize_session_token() -> String {
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    if let Some(token_file) = session_token_path() {
        if let Some(token_dir) = token_file.parent() {
            let _ = std::fs::create_dir_all(token_dir);
        }
        if std::fs::write(&token_file, &token).is_ok() {
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

/// Extracts Bearer token from Authorization header if present
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

/// Pure Bearer Token Authentication Middleware
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    // 0. Preflight OPTIONS requests always pass to CORS layer
    if req.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    let path = req.uri().path().to_string();

    // 1. Extract Bearer Token from Authorization header
    let bearer_token = extract_bearer_token(&req);

    let (is_authenticated, auth_enforced) = {
        let schema = state.permission_schema.read().await;
        let is_authed = if let Some(ref token) = bearer_token {
            let is_session_valid = constant_time_compare(token, &state.session_token);
            let is_configured_valid = schema.api_auth.tokens.iter().any(|t| constant_time_compare(token, t));
            is_session_valid || is_configured_valid
        } else {
            false
        };
        let enforced = schema.api_auth.required;
        (is_authed, enforced)
    };

    // When API Authentication is disabled (required == false in permission.json), allow all routes freely
    if !auth_enforced {
        return Ok(next.run(req).await);
    }

    // 2. Token management
    let is_token_management = path == "/v1/system/auth/token/generate" || path == "/v1/system/auth/token/revoke";

    if is_token_management {
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

    // 4. Public probe: health always passes
    let is_probe = path == "/health";
    if is_probe {
        return Ok(next.run(req).await);
    }

    // 5. Universal API Authentication enforcement:
    if auth_enforced {
        return Err(unauthorized_response(
            "API Authentication is required. Provide a valid Bearer token in the Authorization header.",
        ));
    }

    // 6. When API Auth is not enforced (local dev without tokens), allow naturally
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
    use axum::http::Request;

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("secret_token_123", "secret_token_123"));
        assert!(!constant_time_compare("secret_token_123", "secret_token_456"));
        assert!(!constant_time_compare("short", "longer_string"));
    }

    #[test]
    fn test_extract_bearer_token() {
        let req = Request::builder()
            .header("Authorization", "Bearer cluaiz_test_key_999")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("cluaiz_test_key_999".to_string()));

        let req_raw = Request::builder()
            .header("Authorization", "raw_token_xyz")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req_raw), Some("raw_token_xyz".to_string()));

        let req_none = Request::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req_none), None);
    }

    #[test]
    #[ignore = "writes only in an explicitly supplied isolated CLUAIZ_HOME / CLUAIZ_MEDIA_TEST_HOME"]
    fn test_session_token_persists_in_selected_environment() -> anyhow::Result<()> {
        let home = std::path::PathBuf::from(std::env::var("CLUAIZ_MEDIA_TEST_HOME")?);
        assert_eq!(std::env::var("CLUAIZ_HOME")?, home.to_string_lossy());
        assert!(
            home.is_absolute() && !home.exists(),
            "Fresh isolated home required"
        );
        let token = initialize_session_token();
        assert_eq!(token.len(), 64);
        let path = session_token_path().expect("selected Cluaiz home");
        assert_eq!(
            path.parent(),
            Some(cluaiz_core::EnvironmentManager::current().global_dir.as_path())
        );
        assert_eq!(
            std::fs::read_to_string(path).expect("written session token"),
            token
        );
        Ok(())
    }
}
