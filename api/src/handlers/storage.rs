use axum::{
    response::Json,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn get_temp_media_dir() -> PathBuf {
    cluaiz_core::EnvironmentManager::current().local_dir.join("temp_media")
}

pub async fn get_temp_media_status() -> Json<Value> {
    let temp_dir = get_temp_media_dir();
    let mut total_size = 0;
    let mut file_count = 0;

    if temp_dir.exists() {
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_size += metadata.len();
                        file_count += 1;
                    }
                }
            }
        }
    }

    let size_mb = total_size as f64 / (1024.0 * 1024.0);

    Json(json!({
        "status": "success",
        "file_count": file_count,
        "total_size_bytes": total_size,
        "total_size_mb": format!("{:.2} MB", size_mb)
    }))
}

pub async fn clean_temp_media() -> Json<Value> {
    let temp_dir = get_temp_media_dir();
    
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir); // Recreate empty directory
    }

    Json(json!({
        "status": "success",
        "message": "Temporary media storage cleaned successfully."
    }))
}

pub async fn get_storage_settings() -> Json<Value> {
    let settings = cluaiz_core::Settings::current();
    Json(json!({
        "cleanup_policy": settings.storage_cleanup_policy
    }))
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct StorageSettingsPayload {
    pub cleanup_policy: String,
}

pub async fn update_storage_settings(axum::Json(payload): axum::Json<StorageSettingsPayload>) -> Json<Value> {
    let mut current = cluaiz_core::Settings::current();
    current.storage_cleanup_policy = payload.cleanup_policy;
    let _ = cluaiz_core::Settings::update(current);
    
    Json(json!({
        "status": "success",
        "message": "Storage settings updated."
    }))
}
