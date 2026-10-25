use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

// ─── POST /v1/system/auth/token/generate ─────────────────────────────
pub async fn generate_auth_token(
    State(state): State<Arc<AppState>>,
) -> (axum::http::StatusCode, Json<Value>) {
    let mut schema = state.permission_schema.read().await.clone();
    if schema.api_auth.tokens.len() >= 5 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": "Maximum limit of 5 API keys reached. Revoke an existing key to generate a new one.",
                "tokens": schema.api_auth.tokens
            })),
        );
    }
    let new_token = cluaiz_security::permission_schema::ApiAuth::generate_token();
    schema.api_auth.tokens.push(new_token.clone());
    schema.api_auth.sanitize_tokens();
    let _ = schema.save();
    *state.permission_schema.write().await = schema.clone();

    (
        axum::http::StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "API token successfully generated.",
            "token": new_token,
            "tokens": schema.api_auth.tokens
        })),
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct RevokeTokenRequest {
    pub token: Option<String>,
    pub index: Option<usize>,
}

// ─── POST /v1/system/auth/token/revoke ───────────────────────────────
pub async fn revoke_auth_token(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RevokeTokenRequest>,
) -> (StatusCode, Json<Value>) {
    let mut schema = state.permission_schema.read().await.clone();
    let target = if let Some(idx) = payload.index {
        idx.to_string()
    } else if let Some(ref tok) = payload.token {
        tok.trim().to_string()
    } else {
        return (StatusCode::BAD_REQUEST, Json(json!({
            "status": "error",
            "message": "Either 'token' or 'index' must be provided to revoke.",
            "tokens": schema.api_auth.tokens,
            "api_auth_required": schema.api_auth.required
        })));
    };

    match schema.api_auth.remove_token(&target) {
        Ok(removed) => {
            if schema.api_auth.tokens.is_empty() {
                schema.api_auth.required = false;
                tracing::warn!("⚠️ All API tokens revoked: Automatically disabled api_auth.required to prevent engine lockout.");
            }
            if let Err(e) = schema.save() {
                let current = state.permission_schema.read().await.clone();
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                    "status": "error",
                    "message": format!("Failed to save permission schema: {}", e),
                    "tokens": current.api_auth.tokens,
                    "api_auth_required": current.api_auth.required
                })));
            }
            *state.permission_schema.write().await = schema.clone();

            (StatusCode::OK, Json(json!({
                "status": "success",
                "message": if schema.api_auth.tokens.is_empty() {
                    "API token successfully revoked. API authentication automatically disabled as no keys remain."
                } else {
                    "API token successfully revoked."
                },
                "revoked_token": removed,
                "tokens": schema.api_auth.tokens,
                "api_auth_required": schema.api_auth.required
            })))
        }
        Err(err) => {
            (StatusCode::BAD_REQUEST, Json(json!({
                "status": "error",
                "message": err,
                "tokens": schema.api_auth.tokens,
                "api_auth_required": schema.api_auth.required
            })))
        }
    }
}
