use axum::http::{HeaderMap, StatusCode};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::state::{AppState, PendingPermissionRequest};
use engines::neural_foundry::security::permission_schema::PermissionSchema;

// ── Workspace Jail & Canonicalization Engine (Directive 3) ────────────────
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathVerificationResult {
    /// Path is safely inside the workspace jail
    InsideWorkspace(PathBuf),
    /// Path is outside the workspace jail
    OutsideWorkspace(PathBuf),
}

impl PathVerificationResult {
    pub fn resolved_path(&self) -> &Path {
        match self {
            Self::InsideWorkspace(p) | Self::OutsideWorkspace(p) => p.as_path(),
        }
    }

    pub fn is_inside(&self) -> bool {
        matches!(self, Self::InsideWorkspace(_))
    }
}

pub async fn get_workspace(state: &Arc<AppState>, headers: &HeaderMap) -> PathBuf {
    if let Some(ws_hdr) = headers.get("x-workspace-root").and_then(|h| h.to_str().ok()) {
        let p = PathBuf::from(ws_hdr);
        if p.exists() {
            return p;
        }
    }
    state.current_workspace_dir.read().await.clone()
}

pub fn resolve_workspace_path(
    input_path: &str,
    workspace: &Path,
    is_write: bool,
) -> Result<PathVerificationResult, (StatusCode, Value)> {
    let raw = Path::new(input_path);
    // Mandate R3: Relative paths MUST be joined to workspace_root first, NEVER process CWD!
    let candidate = if raw.is_relative() {
        workspace.join(raw)
    } else {
        raw.to_path_buf()
    };

    let canonical_workspace = match std::fs::canonicalize(workspace) {
        Ok(c) => c,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "status": "error", "message": format!("Failed to canonicalize workspace root '{}': {}", workspace.display(), e) }),
            ));
        }
    };

    let canonical_target = if candidate.exists() {
        match std::fs::canonicalize(&candidate) {
            Ok(c) => c,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "status": "error", "message": format!("Failed to canonicalize path '{}': {}", candidate.display(), e) }),
                ));
            }
        }
    } else if is_write {
        // Find closest existing ancestor directory for non-existent file writes
        let mut curr = candidate.as_path();
        let mut missing_segments = Vec::new();
        while !curr.exists() {
            if let Some(name) = curr.file_name() {
                missing_segments.push(name.to_os_string());
            }
            match curr.parent() {
                Some(p) => curr = p,
                None => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        json!({ "status": "error", "message": format!("Path has no valid parent directory: {}", input_path) }),
                    ));
                }
            }
        }
        let canon_parent = match std::fs::canonicalize(curr) {
            Ok(c) => c,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "status": "error", "message": format!("Failed to canonicalize parent '{}': {}", curr.display(), e) }),
                ));
            }
        };
        let mut target = canon_parent;
        for seg in missing_segments.into_iter().rev() {
            target.push(seg);
        }
        target
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            json!({ "status": "error", "message": format!("File not found: {}", input_path) }),
        ));
    };

    // Both canonical_target and canonical_workspace are canonicalized, so \\?\ prefix matches on Windows!
    if canonical_target.starts_with(&canonical_workspace) {
        Ok(PathVerificationResult::InsideWorkspace(canonical_target))
    } else {
        Ok(PathVerificationResult::OutsideWorkspace(canonical_target))
    }
}

// ── Helper: Permission Guard ──────────────────────────────────────────────
pub async fn check_permission_gate(
    state: &Arc<AppState>,
    action: &str,
    target_verification: &PathVerificationResult,
    target_display: &str,
    caller_header: Option<&str>,
    caller_body: Option<&str>,
    approval_id: Option<&str>,
) -> Result<(), (StatusCode, Value)> {
    let caller = caller_header
        .or(caller_body)
        .unwrap_or("user")
        .to_lowercase();

    let schema = PermissionSchema::load();
    let mode = schema.agent_security_mode.to_lowercase();

    // 1. Full Access Mode: bypasses boundary check and approvals
    if mode == "full_access" {
        return Ok(());
    }

    // 2. Path Outside Workspace Jail Handling (Directive 3)
    if !target_verification.is_inside() {
        // In Strict mode: HARD BLOCK! No bypass.
        if mode == "strict" {
            return Err((
                StatusCode::FORBIDDEN,
                json!({
                    "status": "blocked",
                    "error": "Workspace Boundary Violation",
                    "message": format!("Strict Mode Block: Path '{}' is outside the active workspace jail. Access denied.", target_display)
                }),
            ));
        }

        // In Sandboxed mode: Access outside workspace ALWAYS requires explicit user approval!
        if let Some(appr_id) = approval_id {
            let lock = state.pending_permissions.read().await;
            if let Some(req) = lock.get(appr_id) {
                if req.status == "approved" && req.target == target_display && req.action == action {
                    return Ok(());
                }
            }
        }

        // Register pending approval for outside-workspace access
        let req_id = Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let pending = PendingPermissionRequest {
            id: req_id.clone(),
            action: action.to_string(),
            target: target_display.to_string(),
            caller: caller.clone(),
            timestamp_ms: timestamp,
            status: "pending".to_string(),
        };

        {
            let mut lock = state.pending_permissions.write().await;
            lock.insert(req_id.clone(), pending);
        }

        return Err((
            StatusCode::FORBIDDEN,
            json!({
                "status": "pending_approval",
                "request_id": req_id,
                "action": action,
                "target": target_display,
                "message": format!("Path '{}' is outside the workspace. User approval required.", target_display)
            }),
        ));
    }

    // 3. Inside Workspace Path Handling
    if let Some(appr_id) = approval_id {
        let lock = state.pending_permissions.read().await;
        if let Some(req) = lock.get(appr_id) {
            if req.status == "approved" && req.target == target_display && req.action == action {
                return Ok(());
            }
        }
    }

    // In Sandboxed mode: safe reads inside workspace are allowed without prompt
    if mode == "sandboxed" {
        if action == "fs_read" || action == "fs_list" {
            return Ok(());
        }
        // Direct user action inside workspace is allowed
        if caller != "agent" && caller != "ai" {
            return Ok(());
        }
    }

    // Strict Mode or AI Write inside workspace: require approval
    let req_id = Uuid::new_v4().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let pending = PendingPermissionRequest {
        id: req_id.clone(),
        action: action.to_string(),
        target: target_display.to_string(),
        caller: caller.clone(),
        timestamp_ms: timestamp,
        status: "pending".to_string(),
    };

    {
        let mut lock = state.pending_permissions.write().await;
        lock.insert(req_id.clone(), pending);
    }

    Err((
        StatusCode::FORBIDDEN,
        json!({
            "status": "pending_approval",
            "request_id": req_id,
            "action": action,
            "target": target_display,
            "message": format!("Action '{}' on '{}' requires user approval under '{}' mode.", action, target_display, mode)
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(name_prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{}_{}", name_prefix, uuid::Uuid::new_v4().simple()));
            let _ = fs::create_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_resolve_path_relative_inside_workspace() {
        let ws = TempTestDir::new("cluaiz_test_ws_inside");
        let result = resolve_workspace_path("sub/file.txt", &ws.path, true).expect("Should resolve valid write path");
        assert!(result.is_inside());
        let canonical_ws = fs::canonicalize(&ws.path).unwrap();
        assert!(result.resolved_path().starts_with(&canonical_ws));
    }

    #[test]
    fn test_resolve_path_traversal_escape() {
        let ws = TempTestDir::new("cluaiz_test_ws_escape");
        let result = resolve_workspace_path("../../escaped_secret.txt", &ws.path, true).expect("Should resolve traversal path");
        assert!(!result.is_inside(), "Path traversal must be classified as OutsideWorkspace!");
    }

    #[test]
    fn test_resolve_path_nonexistent_read_returns_not_found() {
        let ws = TempTestDir::new("cluaiz_test_ws_read_404");
        let result = resolve_workspace_path("non_existent_file.txt", &ws.path, false);
        assert!(result.is_err());
        let (status, _val) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_check_permission_gate_and_verify_zero_disk_mutation_on_block() {
        let ws = TempTestDir::new("cluaiz_test_ws_gate");
        let outside = TempTestDir::new("cluaiz_test_outside");
        let state = AppState::new_test_state(ws.path.clone());

        let target_file = outside.path.join("unapproved_evil_file.txt");
        assert!(!target_file.exists(), "Target file must not exist before test");

        let verification = resolve_workspace_path(target_file.to_str().unwrap(), &ws.path, true).unwrap();
        assert!(!verification.is_inside());

        // Under sandboxed mode without approval:
        let gate_res = check_permission_gate(
            &state,
            "fs_write",
            &verification,
            target_file.to_str().unwrap(),
            Some("agent"),
            None,
            None,
        ).await;

        assert!(gate_res.is_err());
        let (status, val) = gate_res.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(val["status"], "pending_approval");

        // 🛡️ DIRECTIVE 5 CRITICAL ASSERTION: Disk MUST NOT be mutated!
        assert!(!target_file.exists(), "Blocked/Pending action must NEVER create or touch target file on disk!");
    }

    #[tokio::test]
    async fn test_approval_resolution_allows_execution() {
        let ws = TempTestDir::new("cluaiz_test_ws_approval");
        let state = AppState::new_test_state(ws.path.clone());

        let target_file = ws.path.join("approved_file.txt");
        let verification = resolve_workspace_path(target_file.to_str().unwrap(), &ws.path, true).unwrap();

        // 1. Initial agent request generates pending permission
        let gate_err = check_permission_gate(
            &state,
            "fs_write",
            &verification,
            target_file.to_str().unwrap(),
            Some("agent"),
            None,
            None,
        ).await.unwrap_err();

        let req_id = gate_err.1["request_id"].as_str().unwrap().to_string();

        // 2. User approves permission
        {
            let mut lock = state.pending_permissions.write().await;
            if let Some(req) = lock.get_mut(&req_id) {
                req.status = "approved".to_string();
            }
        }

        // 3. Retry with valid approval_id succeeds
        let gate_ok = check_permission_gate(
            &state,
            "fs_write",
            &verification,
            target_file.to_str().unwrap(),
            Some("agent"),
            None,
            Some(&req_id),
        ).await;

        assert!(gate_ok.is_ok(), "Approved permission request must successfully pass the gate");
    }
}
