use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

use engines::neural_foundry::security::permission_schema::PermissionSchema;
use crate::state::AppState;

pub async fn auth_middleware(
    State(_state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    let path = req.uri().path();
    
    // 1. Skip authentication for DevHub UI (static files)
    // Most DevHub routes don't start with these prefixes.
    let is_api_route = path.starts_with("/v1/") 
        || path.starts_with("/api/") 
        || path.starts_with("/models/") 
        || path.starts_with("/engine/") 
        || path.starts_with("/hardware");

    if !is_api_route {
        return Ok(next.run(req).await);
    }

    // 2. Whitelist specific API routes that the DevHub needs internally to function
    // For example, if we lock out /v1/system/permission, the user can never turn OFF auth.
    // The UI also uses /api/components, /v1/booster, and /v1/models/installed.
    let is_whitelisted = path.starts_with("/v1/system/permission")
        || path.starts_with("/info")
        || path.starts_with("/health")
        || path.starts_with("/api/components/")
        || path.starts_with("/v1/booster/")
        || path.starts_with("/v1/models/installed")
        || path.starts_with("/v1/skills/remove")
        || path.starts_with("/v1/extensions/remove")
        || path.starts_with("/v1/plugins/remove")
        || path.starts_with("/v1/mcps/remove");

    if is_whitelisted {
        return Ok(next.run(req).await);
    }

    // Load current permissions directly from disk (fast enough for local engine)
    let schema = PermissionSchema::load();

    // If authentication is not required, proceed
    if !schema.api_auth.required {
        return Ok(next.run(req).await);
    }

    // Extract Authorization header
    let auth_header = req.headers().get(axum::http::header::AUTHORIZATION);
    
    let bearer_token = match auth_header {
        Some(header) => match header.to_str() {
            Ok(val) => {
                if val.starts_with("Bearer ") {
                    val.trim_start_matches("Bearer ").trim()
                } else {
                    return Err(unauthorized_response("Invalid Authorization header format. Expected 'Bearer <token>'."));
                }
            }
            Err(_) => return Err(unauthorized_response("Invalid Authorization header string.")),
        },
        None => return Err(unauthorized_response("Missing Authorization header. API Authentication is required.")),
    };

    // Verify if the provided token matches any allowed tokens
    if schema.api_auth.tokens.contains(&bearer_token.to_string()) {
        Ok(next.run(req).await)
    } else {
        Err(unauthorized_response("Invalid Bearer token."))
    }
}

fn unauthorized_response(msg: &str) -> Response {
    let body = Json(json!({
        "error": "Unauthorized",
        "message": msg
    }));
    (StatusCode::UNAUTHORIZED, body).into_response()
}
