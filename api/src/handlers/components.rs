use axum::{Json, extract::{State, Query}, http::StatusCode};
use std::sync::Arc;
use crate::state::AppState;
use serde_json::{json, Value};

pub async fn list_components(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let env = cluaiz_core::EnvironmentManager::current();
    let registry = cluaiz_agents::ToolsEngine::registry().unwrap_or_default();
    let mut results = serde_json::Map::new();
    let mut rich_map = serde_json::Map::new();
    
    for comp_type in ["plugin", "mcp", "skill"] {
        let dir = match comp_type {
            "skill" => env.skills_dir(),
            "plugin" => env.plugins_dir(),
            "mcp" => env.mcp_dir(),
            _ => env.tools_dir().join(format!("{}s", comp_type)),
        };
        let mut names = Vec::new();
        let mut rich_items = Vec::new();
        let get_icon_svg = |base_path: &std::path::Path| -> Option<String> {
            let p1 = base_path.join("assets").join("icon.svg");
            let p2 = base_path.join("icon.svg");
            if p1.exists() {
                std::fs::read_to_string(p1).ok()
            } else if p2.exists() {
                std::fs::read_to_string(p2).ok()
            } else {
                None
            }
        };

        // 1. Scan directory
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let id = entry.file_name().to_string_lossy().to_string();
                        names.push(serde_json::Value::String(id.clone()));
                        
                        let tool_opt = registry.installed_tools.get(&id);
                        let name = tool_opt.map(|t| t.name.clone()).unwrap_or_else(|| id.clone());
                        let desc = tool_opt.map(|t| t.description.clone()).unwrap_or_default();
                        let enabled = tool_opt.map(|t| t.enabled).unwrap_or(true);
                        let sec_mode = tool_opt.map(|t| t.security_mode.as_str().to_string()).unwrap_or_else(|| "inherit".to_string());
                        let version = tool_opt.map(|t| t.version.clone()).unwrap_or_else(|| "1.0.0".to_string());
                        let exec_mode = tool_opt.map(|t| format!("{:?}", t.execution_mode).to_lowercase()).unwrap_or_else(|| "auto".to_string());
                        let triggers = tool_opt.map(|t| t.semantic_triggers.clone()).unwrap_or_default();
                        let tokens = (desc.len() / 4).max(4);
                        let icon_svg = get_icon_svg(&entry.path())
                            .or_else(|| tool_opt.and_then(|t| get_icon_svg(&std::path::PathBuf::from(&t.local_dir))));

                        rich_items.push(serde_json::json!({
                            "id": id,
                            "name": name,
                            "category": comp_type,
                            "version": version,
                            "description": desc,
                            "enabled": enabled,
                            "security_mode": sec_mode,
                            "execution_mode": exec_mode,
                            "semantic_triggers": triggers,
                            "tokens": tokens,
                            "icon_svg": icon_svg,
                        }));
                    }
                }
            }
        }

        // 2. Also merge any tools registered in ToolsRegistry for this category
        for (id, tool) in &registry.installed_tools {
            let id_str: &str = id.as_str();
            if tool.category.as_str() == comp_type && !names.iter().any(|v: &serde_json::Value| v.as_str() == Some(id_str)) {
                names.push(serde_json::Value::String(id.clone()));
                let tokens = (tool.description.len() / 4).max(4);
                let icon_svg = get_icon_svg(&std::path::PathBuf::from(&tool.local_dir));
                rich_items.push(serde_json::json!({
                    "id": id,
                    "name": tool.name,
                    "category": comp_type,
                    "version": tool.version,
                    "description": tool.description,
                    "enabled": tool.enabled,
                    "security_mode": tool.security_mode.as_str(),
                    "execution_mode": format!("{:?}", tool.execution_mode).to_lowercase(),
                    "semantic_triggers": tool.semantic_triggers,
                    "tokens": tokens,
                    "icon_svg": icon_svg,
                }));
            }
        }

        results.insert(comp_type.to_string(), serde_json::Value::Array(names));
        rich_map.insert(comp_type.to_string(), serde_json::Value::Array(rich_items));
    }
    results.insert("rich".to_string(), serde_json::Value::Object(rich_map));

    Json(serde_json::Value::Object(results))
}

/// Installs a registry component and reports the installer's real outcome.
/// The install runs on its own task, so a client that disconnects mid-download
/// does not abort it and leave a half-extracted package behind.
pub(crate) async fn install_component_response(component_type: &'static str, name: String) -> (StatusCode, Json<Value>) {
    if name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "message": format!("A {} name is required.", component_type)})),
        );
    }

    let task_name = name.clone();
    let outcome = tokio::spawn(async move {
        cluaiz_agents::installer::ToolsInstaller::install_component(component_type, &task_name).await
    })
    .await;

    match outcome {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(json!({"status": "success", "message": format!("{} '{}' installed.", component_type, name)})),
        ),
        Ok(Err(e)) => {
            tracing::warn!("Install of {} '{}' failed: {}", component_type, name, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": format!("Failed to install {} '{}': {}", component_type, name, e)})),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status": "error", "message": format!("Install task for {} '{}' stopped unexpectedly: {}", component_type, name, e)})),
        ),
    }
}

fn error_response(status: StatusCode, message: String) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"status": "error", "message": message})))
}

/// Rejects an invalid or not-installed component id with 400 / 404.
fn check_installed(component_type: &str, name: &str) -> Result<(), (StatusCode, Json<Value>)> {
    use cluaiz_agents::installer::ToolsInstaller;
    if !ToolsInstaller::is_valid_component_id(name) {
        return Err(error_response(StatusCode::BAD_REQUEST, format!("Invalid {} name '{}'.", component_type, name)));
    }
    if !ToolsInstaller::is_installed(component_type, name) {
        return Err(error_response(StatusCode::NOT_FOUND, format!("{} '{}' is not installed.", component_type, name)));
    }
    Ok(())
}

/// Uninstalls a component and reports the real outcome.
pub(crate) async fn remove_component_response(component_type: &'static str, name: String) -> (StatusCode, Json<Value>) {
    if let Err(rejected) = check_installed(component_type, &name) {
        return rejected;
    }
    match cluaiz_agents::installer::ToolsInstaller::remove_component(component_type, &name).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"status": "success", "message": format!("{} '{}' removed.", component_type, name)})),
        ),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to remove {} '{}': {}", component_type, name, e)),
    }
}

/// Lists installed components of one type for the `/cache` GET endpoints.
pub(crate) fn list_cache_response(component_type: &str) -> (StatusCode, Json<Value>) {
    match cluaiz_agents::installer::ToolsInstaller::list_component_cache(component_type) {
        Ok(report) => (StatusCode::OK, Json(json!({"status": "success", "message": report}))),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list {} cache: {}", component_type, e)),
    }
}

/// Clears cached files of one component (or all of a type) without uninstalling anything.
pub(crate) fn clear_cache_response(component_type: &str, target: Option<String>, all: bool) -> (StatusCode, Json<Value>) {
    if let Some(name) = &target {
        if let Err(rejected) = check_installed(component_type, name) {
            return rejected;
        }
    }
    match cluaiz_agents::installer::ToolsInstaller::clear_component_cache(component_type, target, all, all) {
        Ok(report) => (StatusCode::OK, Json(json!({
            "status": "success",
            "message": format!("Removed {} cached files.", report.files),
            "files_removed": report.files,
            "freed_bytes": report.bytes
        }))),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to clear {} cache: {}", component_type, e)),
    }
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
    if let Ok(Some(tool)) = cluaiz_agents::ToolsEngine::get_tool(&comp_id) {
        current_values.insert("enabled".to_string(), serde_json::Value::Bool(tool.enabled));
        current_values.insert("security_mode".to_string(), serde_json::to_value(tool.security_mode).unwrap_or(serde_json::json!("inherit")));
        current_values.insert("execution_mode".to_string(), serde_json::to_value(tool.execution_mode).unwrap_or(serde_json::json!("auto")));
    } else {
        // Fallback default for discovered components
        current_values.insert("enabled".to_string(), serde_json::Value::Bool(true));
        current_values.insert("security_mode".to_string(), serde_json::json!("inherit"));
        current_values.insert("execution_mode".to_string(), serde_json::json!("auto"));
    }

    Json(serde_json::json!({
        "status": "success",
        "schema": {},
        "values": current_values
    }))
}

pub async fn update_settings(State(_state): State<Arc<AppState>>, Json(payload): Json<Value>) -> (StatusCode, Json<Value>) {
    let comp_id = payload.get("component_id").and_then(|v| v.as_str()).unwrap_or("").trim();
    let settings = payload.get("settings").and_then(|v| v.as_object());

    if comp_id.is_empty() || settings.is_none() {
        return error_response(StatusCode::BAD_REQUEST, "Missing component_id or settings".to_string());
    }

    let settings_map = settings.unwrap();

    // 1. Sync enabled state directly to ToolsRegistry (tools_registry.json)
    if let Some(enabled_val) = settings_map.get("enabled").and_then(|v| v.as_bool()) {
        if let Err(e) = cluaiz_agents::ToolsEngine::set_tool_enabled(comp_id, enabled_val) {
            tracing::error!("Failed to set tool enabled for '{}': {}", comp_id, e);
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update enabled status for '{}': {}", comp_id, e));
        }
    }

    // 2. Sync security_mode directly to ToolsRegistry (tools_registry.json)
    if let Some(mode_str) = settings_map.get("security_mode").and_then(|v| v.as_str()) {
        if let Some(mode) = cluaiz_agents::SecurityMode::from_raw(mode_str) {
            if let Err(e) = cluaiz_agents::ToolsEngine::set_tool_security_mode(comp_id, mode) {
                tracing::error!("Failed to set security_mode for '{}': {}", comp_id, e);
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update security_mode for '{}': {}", comp_id, e));
            }
        } else {
            return error_response(StatusCode::BAD_REQUEST, format!("Invalid security_mode value: '{}'", mode_str));
        }
    }

    // 3. Sync execution_mode directly to ToolsRegistry (tools_registry.json)
    if let Some(exec_str) = settings_map.get("execution_mode").and_then(|v| v.as_str()) {
        if let Some(exec_mode) = cluaiz_agents::ExecutionMode::from_raw(exec_str) {
            if let Err(e) = cluaiz_agents::ToolsEngine::set_tool_execution_mode(comp_id, exec_mode) {
                tracing::error!("Failed to set execution_mode for '{}': {}", comp_id, e);
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update execution_mode for '{}': {}", comp_id, e));
            }
        } else {
            return error_response(StatusCode::BAD_REQUEST, format!("Invalid execution_mode value: '{}'", exec_str));
        }
    }

    (StatusCode::OK, Json(json!({
        "status": "success",
        "message": format!("Settings for component '{}' updated successfully", comp_id)
    })))
}

/// Folder of an installed component: `~/.cluaiz/tools/<type>s/<id>`, falling back to the
/// legacy `~/.cluaiz/<type>s/<id>` location when the tools folder has no such component.
fn resolve_component_dir(comp_type: &str, comp_id: &str) -> std::path::PathBuf {
    let env = cluaiz_core::EnvironmentManager::current();
    let target_dir = match comp_type {
        "skill" => env.skills_dir().join(comp_id),
        "plugin" => env.plugins_dir().join(comp_id),
        "mcp" => env.mcp_dir().join(comp_id),
        _ => env.tools_dir().join(format!("{}s", comp_type)).join(comp_id),
    };
    if target_dir.exists() {
        target_dir
    } else {
        env.global_dir.join(format!("{}s", comp_type)).join(comp_id)
    }
}

pub async fn update_file(State(_state): State<Arc<AppState>>, Json(payload): Json<Value>) -> (StatusCode, Json<Value>) {
    let comp_type = payload.get("component_type").and_then(|v| v.as_str()).unwrap_or("").trim_end_matches('s');
    let comp_id = payload.get("component_id").and_then(|v| v.as_str()).unwrap_or("");
    let content = payload.get("content").and_then(|v| v.as_str());

    let Some(content) = content.filter(|_| !comp_type.is_empty() && !comp_id.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({
            "status": "error",
            "message": "Missing component_type, component_id, or content"
        })));
    };
    if !cluaiz_agents::installer::ToolsInstaller::is_valid_component_id(comp_id) {
        return (StatusCode::BAD_REQUEST, Json(json!({
            "status": "error",
            "message": format!("Invalid component_id '{}'", comp_id)
        })));
    }

    let comp_dir = resolve_component_dir(comp_type, comp_id);
    let file_path = if comp_type == "skill" {
        comp_dir.join("SKILL.md")
    } else {
        comp_dir.join("package.json")
    };

    if !file_path.exists() {
        return (StatusCode::NOT_FOUND, Json(json!({
            "status": "error",
            "message": format!("File not found at {}", file_path.display())
        })));
    }

    if let Err(e) = std::fs::write(&file_path, content) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "status": "error",
            "message": format!("Failed to write file: {}", e)
        })));
    }

    (StatusCode::OK, Json(json!({"status": "success"})))
}

#[derive(serde::Deserialize)]
pub struct GetFileQuery {
    pub component_type: String,
    pub component_id: String,
    pub file_path: Option<String>,
}

pub async fn get_files(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> (StatusCode, Json<Value>) {
    let comp_type = query.component_type.trim_end_matches('s');
    let comp_dir = resolve_component_dir(comp_type, &query.component_id);

    if !comp_dir.exists() {
        return error_response(StatusCode::NOT_FOUND, "Component directory not found".to_string());
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

    (StatusCode::OK, Json(json!({
        "status": "success",
        "files": files,
        "cache": {
            "temp_bytes": temp_cache_size,
            "all_bytes": all_cache_size
        }
    })))
}

pub async fn get_specific_file(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> (StatusCode, Json<Value>) {
    open_component_in_editor_impl(_state, query).await
}

pub async fn open_component_in_editor(State(_state): State<Arc<AppState>>, Query(query): Query<GetFileQuery>) -> (StatusCode, Json<Value>) {
    open_component_in_editor_impl(_state, query).await
}

async fn open_component_in_editor_impl(_state: Arc<AppState>, query: GetFileQuery) -> (StatusCode, Json<Value>) {
    let env = cluaiz_core::EnvironmentManager::current();
    let comp_type = query.component_type.trim_end_matches('s');
    
    let file_path: Option<std::path::PathBuf> = if comp_type == "model" {
        // Resolve model from models directory across categories (chat, embedding, vision, audio)
        let models_base = env.models_dir();
        let relative = query.file_path.as_deref().unwrap_or("");
        
        let mut found_path: Option<std::path::PathBuf> = None;
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
        let base_dir = match comp_type {
            "skill" => env.skills_dir(),
            "plugin" => env.plugins_dir(),
            "mcp" => env.mcp_dir(),
            _ => env.tools_dir().join(format!("{}s", comp_type)),
        };
        let mut comp_dir = base_dir.join(&query.component_id);
        if !comp_dir.exists() {
            comp_dir = env.global_dir.join(format!("{}s", comp_type)).join(&query.component_id);
        }
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

    let target_file: std::path::PathBuf = match file_path {
        Some(p) if p.exists() => p,
        _ => return error_response(StatusCode::NOT_FOUND, "File not found".to_string()),
    };

    match std::fs::read_to_string(&target_file) {
        Ok(content) => (StatusCode::OK, Json(json!({"status": "success", "content": content}))),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read file: {}", e)),
    }
}

#[derive(serde::Deserialize)]
pub struct ClearCachePayload {
    pub component_type: String,
    pub component_id: String,
    pub all: bool,
}

pub async fn clear_cache(State(_state): State<Arc<AppState>>, Json(payload): Json<ClearCachePayload>) -> (StatusCode, Json<Value>) {
    let comp_type = payload.component_type.trim_end_matches('s');
    let target = (!payload.component_id.is_empty()).then_some(payload.component_id);
    clear_cache_response(comp_type, target, payload.all)
}
