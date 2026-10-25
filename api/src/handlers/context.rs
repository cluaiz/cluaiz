//! The context planner's settings (preview, reports and jobs are CEP `context.*` methods)

use super::chat::context_plan;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": { "message": message.into(), "type": "invalid_request_error" } }))).into_response()
}

/// GET /v1/context/settings: the planner's default policy
pub async fn get_context_settings() -> Json<Value> {
    Json(json!({ "settings": cluaiz_core::Settings::current().context_runtime }))
}

/// POST /v1/context/settings: changes the given fields of the default policy. The first write of
/// this section backs up `settings.json` next to it.
pub async fn update_context_settings(Json(patch): Json<Value>) -> Response {
    let Some(fields) = patch.as_object() else {
        return error(StatusCode::BAD_REQUEST, "settings must be an object");
    };
    let mut settings = cluaiz_core::Settings::current();
    let mut value = serde_json::to_value(&settings.context_runtime).unwrap_or_default();
    for (key, field) in fields {
        if value.get(key).is_none() {
            return error(StatusCode::BAD_REQUEST, format!("unknown field '{key}'"));
        }
        value[key] = field.clone();
    }
    let policy: cluaiz_context::Policy = match serde_json::from_value(value) {
        Ok(policy) if context_plan::valid(&policy) => policy,
        _ => {
            return error(
                StatusCode::BAD_REQUEST,
                "percentages must be whole numbers from 0 to 100 and recent_turns_min at most 50",
            )
        }
    };
    let env = cluaiz_core::EnvironmentManager::current();
    let path = env.config_dir().join("settings.json");
    let has_section = std::fs::read_to_string(&path).is_ok_and(|text| text.contains("\"context_runtime\""));
    if path.exists() && !has_section {
        let backup = path.with_file_name(format!("settings.json.bak-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        if let Err(e) = std::fs::copy(&path, &backup) {
            return error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not back up settings.json: {e}"));
        }
        tracing::info!("🗂️ [Context] Backed up settings.json to {:?}", backup);
    }
    settings.context_runtime = policy.clone();
    if let Err(e) = cluaiz_core::Settings::update(settings) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(json!({ "settings": policy })).into_response()
}
