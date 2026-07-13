use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use engines::neural_foundry::security::permission_schema::PermissionSchema;

// ─── GET /v1/system/permission ───────────────────────────────────────
pub async fn get_permission(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let schema = PermissionSchema::load();
    Json(json!({
        "status": "success",
        "permission": schema
    }))
}

// ─── POST /v1/system/permission ──────────────────────────────────────
pub async fn update_permission(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<PermissionSchema>,
) -> Json<Value> {
    payload.save();
    Json(json!({
        "status": "success",
        "message": "Permission.json successfully updated."
    }))
}
