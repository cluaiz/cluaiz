use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

// ─── GET /v1/skills/list ──────────────────────────────────────────────
pub async fn list_skills(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_installed_components("skill") {
        Ok(skills) => Json(json!({"status": "success", "skills": skills})),
        Err(_) => Json(json!({"status": "success", "skills": []}))
    }
}

#[derive(serde::Deserialize)]
pub struct InstallSkillPayload {
    pub skill_name: String,
}

// ─── POST /v1/skills/install ──────────────────────────────────────────
pub async fn install_skill(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallSkillPayload>
) -> Json<Value> {
    let skill_name = payload.skill_name.clone();
    tokio::spawn(async move {
        let _ = engines::neural_foundry::registry::hub_installer::HubInstaller::install_component("skill", &skill_name).await;
    });

    Json(json!({"status": "success", "message": format!("WASM skill '{}' installation queued natively in Engine.", payload.skill_name)}))
}

// ─── GET /v1/skills/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_component_cache("skill") {
        Ok(report) => Json(json!({"status": "success", "message": report})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to list cache: {}", e)}))
    }
}

// ─── DELETE /v1/skills/cache ──────────────────────────────────────────
pub async fn clear_cache(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::clear_component_cache("skill", None, true, true) {
        Ok(wiped) => Json(json!({"status": "success", "message": format!("Successfully wiped {} caches.", wiped)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to clear cache: {}", e)}))
    }
}

// ─── DELETE /v1/skills/remove ─────────────────────────────────────────
pub async fn remove_skill(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallSkillPayload>
) -> Json<Value> {
    let skill_name = payload.skill_name.clone();
    match engines::neural_foundry::registry::hub_installer::HubInstaller::remove_component("skill", &skill_name).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("WASM skill '{}' removed natively.", skill_name)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to remove skill: {}", e)}))
    }
}
