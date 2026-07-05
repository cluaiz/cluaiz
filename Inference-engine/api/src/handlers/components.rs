use axum::{Json, extract::{State, Query}};
use std::sync::Arc;
use crate::state::AppState;
use serde_json::Value;

pub async fn list_components(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let mut results = serde_json::Map::new();
    
    for comp_type in ["extension", "plugin", "mcp", "skill"] {
        let dir = env.global_dir.join(format!("{}s", comp_type));
        let mut items = Vec::new();
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
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
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let comp_type = query.component_type.trim_end_matches('s');
    let comp_id = query.component_id;
    
    let base_dir = env.global_dir.join(format!("{}s", comp_type));
    let comp_dir = base_dir.join(&comp_id);
    let file_name = if comp_type == "skill" { "SKILL.md".to_string() } else { format!("manifest-{}.yaml", comp_type) };
    let file_path = comp_dir.join(&file_name);

    if !file_path.exists() {
        return Json(serde_json::json!({
            "status": "error",
            "message": format!("Component not found at {}", file_path.display())
        }));
    }

    let content = std::fs::read_to_string(&file_path).unwrap_or_default();
    let yaml_part = if comp_type == "skill" && content.starts_with("---\n") {
        if let Some(end_idx) = content[4..].find("---\n") {
            content[4..end_idx + 4].to_string()
        } else {
            content
        }
    } else {
        content
    };

    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_part).unwrap_or(serde_yaml::Value::Null);
    let mut schema = serde_json::Map::new();
    let mut current_values = serde_json::Map::new();

    // Parse Schema
    if let Some(map) = yaml.as_mapping() {
        if let Some(cfg_schema) = map.get(&serde_yaml::Value::String("settings".to_string())) {
            if let Some(schema_map) = cfg_schema.as_mapping() {
                for (k, v) in schema_map {
                    if let Some(k_str) = k.as_str() {
                        if let Ok(v_json) = serde_json::to_value(v) {
                            schema.insert(k_str.to_string(), v_json);
                        }
                    }
                }
            }
        }
    }

    // Load User Settings
    let user_settings_path = env.global_dir.join("engine").join("config").join("user_settings.yaml");
    if user_settings_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&user_settings_path) {
            if let Ok(user_yaml) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                if let Some(root_map) = user_yaml.as_mapping() {
                    let comp_type_key = serde_yaml::Value::String(format!("{}s", comp_type));
                    if let Some(comp_type_val) = root_map.get(&comp_type_key) {
                        if let Some(comp_map) = comp_type_val.as_mapping() {
                            let id_key = serde_yaml::Value::String(comp_id.clone());
                            if let Some(id_val) = comp_map.get(&id_key) {
                                if let Some(id_settings) = id_val.as_mapping() {
                                    for (k, v) in id_settings {
                                        if let Some(k_str) = k.as_str() {
                                            if let Ok(v_json) = serde_json::to_value(v) {
                                                current_values.insert(k_str.to_string(), v_json);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Json(serde_json::json!({
        "status": "success",
        "schema": schema,
        "values": current_values
    }))
}

pub async fn update_settings(State(_state): State<Arc<AppState>>, Json(payload): Json<Value>) -> Json<Value> {
    let comp_type = payload.get("component_type").and_then(|v| v.as_str()).unwrap_or("").trim_end_matches('s');
    let comp_id = payload.get("component_id").and_then(|v| v.as_str()).unwrap_or("");
    let settings = payload.get("settings").and_then(|v| v.as_object());

    if comp_type.is_empty() || comp_id.is_empty() || settings.is_none() {
        return Json(serde_json::json!({
            "status": "error",
            "message": "Missing component_type, component_id, or settings"
        }));
    }

    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let user_settings_path = env.global_dir.join("engine").join("config").join("user_settings.yaml");
    
    let mut user_settings: serde_yaml::Value = if user_settings_path.exists() {
        let content = std::fs::read_to_string(&user_settings_path).unwrap_or_default();
        serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
    } else {
        if let Some(p) = user_settings_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };

    if let Some(root_map) = user_settings.as_mapping_mut() {
        let comp_type_key = serde_yaml::Value::String(format!("{}s", comp_type));
        if !root_map.contains_key(&comp_type_key) {
            root_map.insert(comp_type_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
        }
        
        if let Some(comp_type_map_val) = root_map.get_mut(&comp_type_key) {
            if let Some(comp_type_map) = comp_type_map_val.as_mapping_mut() {
                let id_key = serde_yaml::Value::String(comp_id.to_string());
                if !comp_type_map.contains_key(&id_key) {
                    comp_type_map.insert(id_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                }
                
                if let Some(id_map_val) = comp_type_map.get_mut(&id_key) {
                    if let Some(id_map) = id_map_val.as_mapping_mut() {
                        for (k, v) in settings.unwrap() {
                            let final_key = serde_yaml::Value::String(k.clone());
                            let parsed_val = match v {
                                Value::Bool(b) => serde_yaml::Value::Bool(*b),
                                Value::Number(n) => if let Some(i) = n.as_i64() { serde_yaml::Value::Number(i.into()) } else if let Some(f) = n.as_f64() { serde_yaml::Value::Number(f.into()) } else { serde_yaml::Value::Null },
                                Value::String(s) => serde_yaml::Value::String(s.clone()),
                                _ => serde_yaml::Value::Null,
                            };
                            id_map.insert(final_key, parsed_val);
                        }
                    }
                }
            }
        }
    }

    if let Err(e) = std::fs::write(&user_settings_path, serde_yaml::to_string(&user_settings).unwrap_or_default()) {
        return Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to write user_settings.yaml: {}", e)
        }));
    }

    Json(serde_json::json!({"status": "success"}))
}
