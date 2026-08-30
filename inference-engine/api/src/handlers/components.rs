use axum::{Json, extract::{State, Query}};
use std::sync::Arc;
use crate::state::AppState;
use serde_json::Value;

pub async fn list_components(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let mut results = serde_json::Map::new();
    
    for comp_type in ["plugin", "mcp", "skill"] {
        let dir = match comp_type {
            "skill" => env.skills_dir(),
            "plugin" => env.plugins_dir(),
            "mcp" => env.mcp_dir(),
            _ => env.tools_dir().join(format!("{}s", comp_type)),
        };
        let mut items = Vec::new();
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        items.push(serde_json::Value::String(entry.file_name().to_string_lossy().to_string()));
                    }
                }
            }
        }
        results.insert(comp_type.to_string(), serde_json::Value::Array(items));
    }
    
    Json(serde_json::Value::Object(results))
}

#[derive(serde::Deserialize)]
pub struct SettingsQuery {
    pub component_type: String,
    pub component_id: String,
}

pub async fn get_settings(State(_state): State<Arc<AppState>>, Query(query): Query<SettingsQuery>) -> Json<Value> {
    let comp_id = query.component_id;
    let mut current_values = serde_json::Map::new();

    // Query tool directly from ToolsRegistry (tools_registry.json)
    if let Ok(Some(tool)) = engines::tools::ToolsEngine::get_tool(&comp_id) {
        current_values.insert("enabled".to_string(), serde_json::Value::Bool(tool.enabled));
        current_values.insert("security_mode".to_string(), serde_json::to_value(&tool.security_mode).unwrap_or(serde_json::json!("full_access")));
        current_values.insert("execution_mode".to_string(), serde_json::to_value(&tool.execution_mode).unwrap_or(serde_json::json!("auto")));
    }

    Json(serde_json::json!({
        "status": "success",
        "schema": {},
        "values": current_values
    }))
}

pub async fn update_settings(State(_state): State<Arc<AppState>>, Json(payload): Json<Value>) -> Json<Value> {
    let comp_id = payload.get("component_id").and_then(|v| v.as_str()).unwrap_or("");
    let settings = payload.get("settings").and_then(|v| v.as_object());

    if comp_id.is_empty() || settings.is_none() {
        return Json(serde_json::json!({
            "status": "error",
            "message": "Missing component_id or settings"
        }));
    }

    let settings_map = settings.unwrap();

    // Sync enabled state directly to ToolsRegistry (tools_registry.json)
    if let Some(enabled_val) = settings_map.get("enabled").and_then(|v| v.as_bool()) {
        let _ = engines::tools::ToolsEngine::set_tool_enabled(comp_id, enabled_val);
    }

    // Sync security_mode directly to ToolsRegistry (tools_registry.json)
    if let Some(mode_str) = settings_map.get("security_mode").and_then(|v| v.as_str()) {
        if let Ok(mode) = serde_json::from_value::<engines::tools::SecurityMode>(serde_json::json!(mode_str)) {
            let _ = engines::tools::ToolsEngine::set_tool_security_mode(comp_id, mode);
        }
    }

    Json(serde_json::json!({"status": "success"}))
}

pub async fn update_file(State(_state): State<Arc<AppState>>, Json(payload): Json<Value>) -> Json<Value> {
    let comp_type = payload.get("component_type").and_then(|v| v.as_str()).unwrap_or("").trim_end_matches('s');
    let comp_id = payload.get("component_id").and_then(|v| v.as_str()).unwrap_or("");
    let content = payload.get("content").and_then(|v| v.as_str());

    if comp_type.is_empty() || comp_id.is_empty() || content.is_none() {
        return Json(serde_json::json!({
            "status": "error",
            "message": "Missing component_type, component_id, or content"
        }));
    }

    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let base_dir = env.global_dir.join(format!("{}s", comp_type));
    let comp_dir = base_dir.join(comp_id);
    let file_path = if comp_type == "skill" {
        comp_dir.join("SKILL.md")
    } else {
        comp_dir.join("package.json")
    };

    if !file_path.exists() {
        return Json(serde_json::json!({
            "status": "error",
            "message": format!("File not found at {}", file_path.display())
        }));
    }

    if let Err(e) = std::fs::write(&file_path, content.unwrap()) {
        return Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write file: {}", e)
        }));
    }

    Json(serde_json::json!({"status": "success"}))
}

#[derive(serde::Deserialize)]
pub struct GetFileQuery {
    pub component_type: String,
    pub component_id: String,
    pub file_path: Option<String>,
}

pub async fn get_files(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> Json<Value> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let comp_type = query.component_type.trim_end_matches('s');
    let comp_dir = env.global_dir.join(format!("{}s", comp_type)).join(&query.component_id);

    if !comp_dir.exists() {
        return Json(serde_json::json!({"status": "error", "message": "Component directory not found"}));
    }

    fn build_tree(dir: &std::path::Path, base: &std::path::Path) -> Vec<Value> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(rel) = path.strip_prefix(base) {
                    let is_dir = path.is_dir();
                    files.push(serde_json::json!({
                        "name": entry.file_name().to_string_lossy().to_string(),
                        "path": rel.to_string_lossy().replace("\\", "/"),
                        "is_dir": is_dir
                    }));
                    if is_dir {
                        files.extend(build_tree(&path, base));
                    }
                }
            }
        }
        files
    }

    let files = build_tree(&comp_dir, &comp_dir);

    let mut temp_cache_size = 0;
    let mut all_cache_size = 0;
    let cache_dir = comp_dir.join(".cache");
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    let size = metadata.len();
                    all_cache_size += size;
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.starts_with("temp_") || file_name.ends_with(".tmp") {
                        temp_cache_size += size;
                    }
                }
            }
        }
    }

    Json(serde_json::json!({
        "status": "success",
        "files": files,
        "cache": {
            "temp_bytes": temp_cache_size,
            "all_bytes": all_cache_size
        }
    }))
}

pub async fn get_specific_file(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> Json<Value> {
    open_component_in_editor_impl(_state, query).await
}

pub async fn open_component_in_editor(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> Json<Value> {
    open_component_in_editor_impl(_state, query).await
}

async fn open_component_in_editor_impl(_state: Arc<AppState>, query: GetFileQuery) -> Json<Value> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let comp_type = query.component_type.trim_end_matches('s');
    
    let file_path = if comp_type == "model" {
        // Resolve model from models directory across categories (chat, embedding, vision, audio)
        let models_base = env.models_dir();
        let relative = query.file_path.as_deref().unwrap_or("");
        
        let mut found_path = None;
        let categories = ["embedding", "chat", "vision", "audio", "image_gen"];
        for cat in &categories {
            let candidate = models_base.join(cat).join(&query.component_id).join(relative);
            if candidate.exists() {
                found_path = Some(candidate);
                break;
            }
        }
        found_path
    } else {
        let comp_dir = env.global_dir.join(format!("{}s", comp_type)).join(&query.component_id);
        if let Some(p) = &query.file_path {
            Some(comp_dir.join(p))
        } else {
            if comp_type == "skill" {
                Some(comp_dir.join("SKILL.md"))
            } else {
                Some(comp_dir.join("package.json"))
            }
        }
    };

    let target_file = match file_path {
        Some(p) if p.exists() => p,
        _ => return Json(serde_json::json!({"status": "error", "message": "File not found"})),
    };

    let content = std::fs::read_to_string(&target_file).unwrap_or_default();
    Json(serde_json::json!({"status": "success", "content": content}))
}

#[derive(serde::Deserialize)]
pub struct ClearCachePayload {
    pub component_type: String,
    pub component_id: String,
    pub all: bool,
}

pub async fn clear_cache(State(_state): State<Arc<AppState>>, Json(payload): Json<ClearCachePayload>) -> Json<Value> {
    let comp_type = payload.component_type.trim_end_matches('s');
    match engines::tools::ToolHubInstaller::clear_component_cache(
        comp_type,
        Some(payload.component_id.clone()),
        payload.all,
        true
    ) {
        Ok(wiped) => Json(serde_json::json!({"status": "success", "message": format!("Successfully wiped {} caches.", wiped)})),
        Err(e) => Json(serde_json::json!({"status": "error", "message": format!("Failed to clear cache: {}", e)}))
    }
}
