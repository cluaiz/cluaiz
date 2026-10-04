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
    let models_root = engine_core::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| engine_core::environment::EnvironmentManager::current().models_dir());

    let registry = engines::models::InstalledStateRegistry::load();

    let mut available_chat_models: Vec<String> = Vec::new();
    let mut available_vector_models: Vec<String> = Vec::new();
    let mut available_vision_models: Vec<String> = Vec::new();
    let mut available_audio_models: Vec<String> = Vec::new();
    let mut available_text_embedding_models: Vec<String> = Vec::new();
    let mut available_vision_embedding_models: Vec<String> = Vec::new();
    let mut available_vision_ingest_models: Vec<String> = Vec::new();
    let mut available_tts_models: Vec<String> = Vec::new();
    let mut available_stt_models: Vec<String> = Vec::new();
    let mut all_models: Vec<String> = Vec::new();

    // Dynamically categorize strictly from 5 Sovereign Vault categories
    for (id, entry) in &registry.installed_models {
        all_models.push(id.clone());
        match entry.category.as_str() {
            "chat" => available_chat_models.push(id.clone()),
            "embedding" | "text-embedding" => {
                available_text_embedding_models.push(id.clone());
                available_vector_models.push(id.clone());
            }
            "vision-embedding" => {
                available_vision_embedding_models.push(id.clone());
                available_vector_models.push(id.clone());
            }
            "ingest" | "vision-ingest" | "vision" => {
                available_vision_ingest_models.push(id.clone());
                available_vision_models.push(id.clone());
            }
            "tts" => {
                available_tts_models.push(id.clone());
                available_audio_models.push(id.clone());
            }
            "stt" => {
                available_stt_models.push(id.clone());
                available_audio_models.push(id.clone());
            }
            _ => available_chat_models.push(id.clone()), // fallback
        }
    }

    // Check if active slots configuration points to models that no longer exist on disk (synced with registry state)
    let mut changed = false;
    let mut active_slots = schema.active_slots.clone();
    
    for (slot_name, slot_config) in active_slots.iter_mut() {
        if let Some(ref model_id) = slot_config.model_id {
            if !registry.installed_models.contains_key(model_id) {
                // Model was manually deleted or unregistered. Reset slot safely.
                slot_config.model_id = None;
                slot_config.format_type = None;
                slot_config.supported_tasks = Vec::new();
                changed = true;
            }
        }
    }

    let mut schema = schema;
    if changed {
        schema.active_slots = active_slots;
        let _ = schema.save(); // Persist clean state to Permission.json
    }

    let lan_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    
    // Inject available list properties into permission JSON so UI gets them
    let mut perm_json = serde_json::to_value(&schema).unwrap_or(json!({}));
    if let Some(obj) = perm_json.as_object_mut() {
        obj.insert("available_models".to_string(), json!(all_models));
        obj.insert("available_chat_models".to_string(), json!(available_chat_models));
        obj.insert("available_vector_models".to_string(), json!(available_vector_models));
        obj.insert("available_vision_models".to_string(), json!(available_vision_models));
        obj.insert("available_audio_models".to_string(), json!(available_audio_models));
        obj.insert("available_text_embedding_models".to_string(), json!(available_text_embedding_models));
        obj.insert("available_vision_embedding_models".to_string(), json!(available_vision_embedding_models));
        obj.insert("available_vision_ingest_models".to_string(), json!(available_vision_ingest_models));
        obj.insert("available_tts_models".to_string(), json!(available_tts_models));
        obj.insert("available_stt_models".to_string(), json!(available_stt_models));
    }

    Json(json!({
        "status": "success",
        "permission": perm_json,
        "lan_ip": lan_ip
    }))
}

// ─── POST /v1/system/permission ──────────────────────────────────────
pub async fn update_permission(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let mut schema = PermissionSchema::load();
    schema.merge_patch(&payload);
    if let Err(e) = schema.save() {
        tracing::error!("❌ Failed to save permission schema to disk: {:?}", e);
        return Json(json!({
            "status": "error",
            "message": format!("Failed to save permission schema: {}", e),
            "permission": serde_json::to_value(&schema).unwrap_or(json!({}))
        }));
    }
    
    Json(json!({
        "status": "success",
        "message": "permission.json successfully updated.",
        "permission": serde_json::to_value(&schema).unwrap_or(json!({}))
    }))
}

// ─── POST /v1/system/auth/token/generate ─────────────────────────────
pub async fn generate_auth_token(
    State(_state): State<Arc<AppState>>,
) -> (axum::http::StatusCode, Json<Value>) {
    let mut schema = PermissionSchema::load();
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
    let new_token = engines::neural_foundry::security::permission_schema::ApiAuth::generate_token();
    schema.api_auth.tokens.push(new_token.clone());
    schema.api_auth.sanitize_tokens();
    let _ = schema.save();

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
    pub token: String,
}

// ─── POST /v1/system/auth/token/revoke ───────────────────────────────
pub async fn revoke_auth_token(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<RevokeTokenRequest>,
) -> Json<Value> {
    let mut schema = PermissionSchema::load();
    let target = payload.token.trim();
    schema.api_auth.tokens.retain(|t| t.trim() != target);
    schema.api_auth.sanitize_tokens();
    if schema.api_auth.tokens.is_empty() {
        schema.api_auth.required = false;
        tracing::warn!("⚠️ All API tokens revoked: Automatically disabled api_auth.required to prevent engine lockout.");
    }
    let _ = schema.save();

    Json(json!({
        "status": "success",
        "message": if schema.api_auth.tokens.is_empty() {
            "API token successfully revoked. API authentication automatically disabled as no keys remain."
        } else {
            "API token successfully revoked."
        },
        "tokens": schema.api_auth.tokens,
        "api_auth_required": schema.api_auth.required
    }))
}


// ─── Real-Time Human-In-The-Loop Approval Endpoints ──────────────────
#[derive(Debug, serde::Deserialize)]
pub struct ApprovalActionRequest {
    pub request_id: String,
    pub feedback: Option<String>,
}

pub async fn get_pending_permissions(
    State(state): State<Arc<AppState>>,
) -> (axum::http::StatusCode, Json<Value>) {
    let lock = state.pending_permissions.read().await;
    let items: Vec<crate::state::PendingPermissionRequest> = lock.values().cloned().collect();
    (
        axum::http::StatusCode::OK,
        Json(json!({
            "status": "success",
            "pending": items
        })),
    )
}

pub async fn approve_permission(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ApprovalActionRequest>,
) -> (axum::http::StatusCode, Json<Value>) {
    // 1. Signal active streaming task oneshot channel (Real-Time HITL resume)
    {
        let mut approvals_lock = state.active_approvals.write().await;
        if let Some(tx) = approvals_lock.remove(&payload.request_id) {
            let _ = tx.send(crate::state::ApprovalDecision {
                approved: true,
                feedback: payload.feedback.clone(),
            });
        }
    }

    // 2. Update persistent pending_permissions
    let mut lock = state.pending_permissions.write().await;
    if let Some(req) = lock.get_mut(&payload.request_id) {
        req.status = "approved".to_string();
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Permission approved",
                "request": req
            })),
        )
    } else {
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Approval signal dispatched"
            })),
        )
    }
}

pub async fn reject_permission(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ApprovalActionRequest>,
) -> (axum::http::StatusCode, Json<Value>) {
    // 1. Signal active streaming task oneshot channel (Real-Time HITL resume)
    {
        let mut approvals_lock = state.active_approvals.write().await;
        if let Some(tx) = approvals_lock.remove(&payload.request_id) {
            let _ = tx.send(crate::state::ApprovalDecision {
                approved: false,
                feedback: payload.feedback.clone(),
            });
        }
    }

    // 2. Update persistent pending_permissions
    let mut lock = state.pending_permissions.write().await;
    if let Some(req) = lock.get_mut(&payload.request_id) {
        req.status = "rejected".to_string();
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Permission rejected",
                "request": req
            })),
        )
    } else {
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Rejection signal dispatched"
            })),
        )
    }
}
