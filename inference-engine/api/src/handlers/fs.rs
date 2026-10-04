use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ReadFileRequest {
    pub path: String,
    #[serde(default)]
    pub is_binary: bool,
    pub caller: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub is_binary: bool,
    pub caller: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListDirRequest {
    pub dir_path: Option<String>,
    pub root_path: Option<String>,
    #[serde(default)]
    pub recursive: bool,
    pub caller: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PathRequest {
    pub path: String,
    pub caller: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub old_path: String,
    pub new_path: String,
    pub caller: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CopyRequest {
    pub src_path: String,
    pub dest_path: String,
    pub caller: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskItemDto {
    pub path: String,
    pub name: String,
    #[serde(rename = "isFolder")]
    pub is_folder: bool,
    pub length: u64,
    pub modified: String,
}



#[derive(Debug, Deserialize)]
pub struct SetWorkspaceRequest {
    pub workspace: String,
}

use crate::handlers::fs_jail::{
    check_permission_gate, get_workspace, resolve_workspace_path, PathVerificationResult,
};

fn extract_caller<'a>(headers: &'a HeaderMap, body_caller: &'a Option<String>) -> Option<&'a str> {
    headers
        .get("x-caller")
        .and_then(|h: &HeaderValue| h.to_str().ok())
        .or(body_caller.as_deref())
}

fn strip_verbatim(p: &Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if s.starts_with(r"\\?\") {
        std::path::PathBuf::from(&s[4..])
    } else {
        p.to_path_buf()
    }
}

// ── Workspace Management Endpoints ────────────────────────────────────────
pub async fn get_workspace_dir(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<Value>) {
    let ws = state.current_workspace_dir.read().await;
    let clean = strip_verbatim(&ws);
    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "workspace": clean.to_string_lossy()
        }))
    )
}

pub async fn set_workspace_dir(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetWorkspaceRequest>,
) -> (StatusCode, Json<Value>) {
    let path = Path::new(&payload.workspace);
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": format!("Workspace directory '{}' does not exist", payload.workspace)
            }))
        );
    }
    match std::fs::canonicalize(path) {
        Ok(canon) => {
            let clean = strip_verbatim(&canon);
            let mut ws = state.current_workspace_dir.write().await;
            *ws = clean.clone();
            (
                StatusCode::OK,
                Json(json!({
                    "status": "success",
                    "workspace": clean.to_string_lossy()
                }))
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "message": format!("Failed to canonicalize workspace: {}", e)
            }))
        ),
    }
}

// ── 1. Read File ──────────────────────────────────────────────────────────
pub async fn read_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ReadFileRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let verification = match resolve_workspace_path(&payload.path, &ws, false) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_read",
        &verification,
        &payload.path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let path = verification.resolved_path();
    if !path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "status": "error",
                "message": format!("File not found: {}", payload.path)
            })),
        );
    }

    if payload.is_binary {
        match fs::read(path) {
            Ok(bytes) => {
                let encoded = BASE64_STANDARD.encode(&bytes);
                (
                    StatusCode::OK,
                    Json(json!({
                        "status": "success",
                        "content": encoded,
                        "is_binary": true
                    })),
                )
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": format!("Failed to read binary file: {}", e)
                })),
            ),
        }
    } else {
        match fs::read_to_string(path) {
            Ok(content) => (
                StatusCode::OK,
                Json(json!({
                    "status": "success",
                    "content": content,
                    "is_binary": false
                })),
            ),
            Err(_) => {
                match fs::read(path) {
                    Ok(bytes) => {
                        let encoded = BASE64_STANDARD.encode(&bytes);
                        (
                            StatusCode::OK,
                            Json(json!({
                                "status": "success",
                                "content": encoded,
                                "is_binary": true
                            })),
                        )
                    }
                    Err(e) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "status": "error",
                            "message": format!("Failed to read file: {}", e)
                        })),
                    ),
                }
            }
        }
    }
}

// ── 2. Write File ─────────────────────────────────────────────────────────
pub async fn write_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<WriteFileRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let verification = match resolve_workspace_path(&payload.path, &ws, true) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_write",
        &verification,
        &payload.path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let path = verification.resolved_path();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    if payload.is_binary {
        match BASE64_STANDARD.decode(&payload.content) {
            Ok(bytes) => match fs::write(path, bytes) {
                Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "status": "error", "message": format!("Write error: {}", e) })),
                ),
            },
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid base64 payload: {}", e) })),
            ),
        }
    } else {
        match fs::write(path, payload.content.as_bytes()) {
            Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": format!("Write error: {}", e) })),
            ),
        }
    }
}

// ── 3. List Directory ─────────────────────────────────────────────────────
pub async fn list_dir(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ListDirRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let target_dir_str = payload
        .dir_path
        .clone()
        .or_else(|| payload.root_path.clone())
        .unwrap_or_else(|| ".".to_string());

    let verification = match resolve_workspace_path(&target_dir_str, &ws, false) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_list",
        &verification,
        &target_dir_str,
        caller_hdr,
        payload.caller.as_deref(),
        None,
    ).await {
        return (status, Json(val));
    }

    let dir = verification.resolved_path();
    if !dir.exists() {
        return (StatusCode::OK, Json(json!({ "status": "success", "items": [] })));
    }

    let payload_root = payload.root_path.as_deref().map(Path::new);
    let base_root = payload_root.unwrap_or(&ws);

    let strip_verbatim = |p: &Path| -> std::path::PathBuf {
        let s = p.to_string_lossy();
        if s.starts_with(r"\\?\") {
            std::path::PathBuf::from(&s[4..])
        } else {
            p.to_path_buf()
        }
    };
    let clean_base_root = strip_verbatim(base_root);

    let mut results: Vec<DiskItemDto> = Vec::new();

    if !payload.recursive {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let clean_entry = strip_verbatim(&entry_path);
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                let rel_str = if let Ok(rel) = clean_entry.strip_prefix(&clean_base_root) {
                    rel.to_string_lossy().replace('\\', "/")
                } else if let Ok(rel) = entry_path.strip_prefix(base_root) {
                    rel.to_string_lossy().replace('\\', "/")
                } else {
                    name.to_string()
                };

                let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                let length = if is_dir { 0 } else { entry.metadata().map(|m| m.len()).unwrap_or(0) };
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_default();

                results.push(DiskItemDto {
                    path: rel_str,
                    name: name.to_string(),
                    is_folder: is_dir,
                    length,
                    modified,
                });
            }
        }
        return (StatusCode::OK, Json(json!({ "status": "success", "items": results })));
    }

    fn walk_dir(
        dir: &Path,
        root: &Path,
        results: &mut Vec<DiskItemDto>,
        ignored: &[&str],
        depth: usize,
    ) {
        if depth > 12 {
            return;
        }
        let strip_verbatim = |p: &Path| -> std::path::PathBuf {
            let s = p.to_string_lossy();
            if s.starts_with(r"\\?\") {
                std::path::PathBuf::from(&s[4..])
            } else {
                p.to_path_buf()
            }
        };
        let clean_root = strip_verbatim(root);

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let clean_entry = strip_verbatim(&entry_path);
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                let rel_str = if let Ok(rel) = clean_entry.strip_prefix(&clean_root) {
                    rel.to_string_lossy().replace('\\', "/")
                } else if let Ok(rel) = entry_path.strip_prefix(root) {
                    rel.to_string_lossy().replace('\\', "/")
                } else {
                    name.to_string()
                };

                let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                let length = if is_dir { 0 } else { entry.metadata().map(|m| m.len()).unwrap_or(0) };
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_default();

                results.push(DiskItemDto {
                    path: rel_str,
                    name: name.to_string(),
                    is_folder: is_dir,
                    length,
                    modified,
                });

                if is_dir {
                    walk_dir(&entry_path, root, results, ignored, depth + 1);
                }
            }
        }
    }

    walk_dir(dir, base_root, &mut results, &[], 0);
    (StatusCode::OK, Json(json!({ "status": "success", "items": results })))
}

// ── 4. Delete Path ────────────────────────────────────────────────────────
pub async fn delete_path(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PathRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let verification = match resolve_workspace_path(&payload.path, &ws, false) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_delete",
        &verification,
        &payload.path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let p = verification.resolved_path();
    if !p.exists() {
        return (StatusCode::OK, Json(json!({ "status": "success", "message": "Already removed" })));
    }

    let res = if p.is_dir() {
        fs::remove_dir_all(p)
    } else {
        fs::remove_file(p)
    };

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": format!("Delete error: {}", e) })),
        ),
    }
}

// ── 5. Rename Path ────────────────────────────────────────────────────────
pub async fn rename_path(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RenameRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let src_verification = match resolve_workspace_path(&payload.old_path, &ws, false) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };
    let dest_verification = match resolve_workspace_path(&payload.new_path, &ws, true) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_rename",
        &src_verification,
        &payload.old_path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_rename",
        &dest_verification,
        &payload.new_path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let old_p = src_verification.resolved_path();
    let new_p = dest_verification.resolved_path();

    if let Some(parent) = new_p.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    match fs::rename(old_p, new_p) {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": format!("Rename error: {}", e) })),
        ),
    }
}

// ── 6. Copy Path ──────────────────────────────────────────────────────────
pub async fn copy_path(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CopyRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let src_verification = match resolve_workspace_path(&payload.src_path, &ws, false) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };
    let dest_verification = match resolve_workspace_path(&payload.dest_path, &ws, true) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_copy",
        &src_verification,
        &payload.src_path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_copy",
        &dest_verification,
        &payload.dest_path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let src = src_verification.resolved_path();
    let dest = dest_verification.resolved_path();

    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    if src.is_dir() {
        fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_dir() {
                    copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
                } else {
                    fs::copy(entry.path(), dst.join(entry.file_name()))?;
                }
            }
            Ok(())
        }

        match copy_dir_all(src, dest) {
            Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": format!("Directory copy error: {}", e) })),
            ),
        }
    } else {
        match fs::copy(src, dest) {
            Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": format!("File copy error: {}", e) })),
            ),
        }
    }
}

// ── 7. Create Directory ───────────────────────────────────────────────────
pub async fn create_dir(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PathRequest>,
) -> (StatusCode, Json<Value>) {
    let ws = get_workspace(&state, &headers).await;
    let verification = match resolve_workspace_path(&payload.path, &ws, true) {
        Ok(v) => v,
        Err((status, val)) => return (status, Json(val)),
    };

    let caller_hdr = extract_caller(&headers, &payload.caller);
    if let Err((status, val)) = check_permission_gate(
        &state,
        "fs_mkdir",
        &verification,
        &payload.path,
        caller_hdr,
        payload.caller.as_deref(),
        payload.approval_id.as_deref(),
    ).await {
        return (status, Json(val));
    }

    let p = verification.resolved_path();
    match fs::create_dir_all(p) {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": format!("Create dir error: {}", e) })),
        ),
    }
}
