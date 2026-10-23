use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_agents::installer::ToolsInstaller;
use super::components::{clear_cache_response, install_component_response, list_cache_response, remove_component_response};

// ─── GET /v1/skills/list ──────────────────────────────────────────────
pub async fn list_skills(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match ToolsInstaller::list_installed_components("skill") {
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
) -> (StatusCode, Json<Value>) {
    install_component_response("skill", payload.skill_name).await
}

// ─── GET /v1/skills/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    list_cache_response("skill")
}

// ─── DELETE /v1/skills/cache ──────────────────────────────────────────
pub async fn clear_cache(State(_state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    clear_cache_response("skill", None, true)
}

// ─── DELETE /v1/skills/remove ─────────────────────────────────────────
pub async fn remove_skill(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallSkillPayload>
) -> (StatusCode, Json<Value>) {
    remove_component_response("skill", payload.skill_name).await
}

