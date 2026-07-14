use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use engines::neural_foundry::security::permission_schema::PermissionSchema;

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };
    match socket.connect("8.8.8.8:80") {
        Ok(()) => match socket.local_addr() {
            Ok(addr) => Some(addr.ip().to_string()),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

// ─── GET /v1/system/permission ───────────────────────────────────────
pub async fn get_permission(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let schema = PermissionSchema::load();
    let lan_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    Json(json!({
        "status": "success",
        "permission": schema,
        "lan_ip": lan_ip
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
