//! One approval gate for every way the agent runs a tool (streaming and non-streaming).
//!
//! `assess` decides whether a call needs the user's approval. `request` registers it where the
//! App sees it (`/v1/system/permission/pending`, and the SSE `permission_request` chunk when
//! streaming), and `wait` holds the call until the user approves, rejects or the time runs out.

use crate::state::{AppState, ApprovalDecision, PendingPermissionRequest};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;
use tokio::sync::oneshot;

/// How long a tool call waits for the user's decision.
pub const APPROVAL_TIMEOUT_SECS: u64 = 120;

/// The chat and tool of each pending approval, so an approval can become a grant
static REQUESTS: LazyLock<Mutex<HashMap<String, (String, String)>>> = LazyLock::new(Default::default);

fn grants() -> std::sync::MutexGuard<'static, cluaiz_agents::lifecycle::grants::Grants> {
    cluaiz_agents::lifecycle::grants::GRANTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The chat's grants, read back from its log the first time after a restart
fn chat_grants(chat: &str) -> std::sync::MutexGuard<'static, cluaiz_agents::lifecycle::grants::Grants> {
    if grants().needs_replay(chat) {
        let records = grant_records(chat);
        grants().replay(chat, &records, Instant::now(), std::time::SystemTime::now());
    }
    grants()
}

/// The chat's `grant` events, oldest first
fn grant_records(chat: &str) -> Vec<cluaiz_agents::lifecycle::grants::GrantRecord> {
    use cluaiz_agents::memory::EventStore;
    let Ok(log) = cluaiz_agents::memory::store_for(chat).read_shared(chat) else { return Vec::new() };
    log.events
        .iter()
        .filter(|event| event.kind == "grant")
        .filter_map(|event| {
            let at = chrono::DateTime::parse_from_rfc3339(&event.ts).ok()?;
            Some(cluaiz_agents::lifecycle::grants::GrantRecord {
                tool: event.data.get("tool").and_then(|t| t.as_str()).map(str::to_string),
                minutes: event.data.get("minutes").and_then(|m| m.as_u64()).unwrap_or(0),
                at: std::time::SystemTime::from(at),
                revoked: event.data.get("revoked").and_then(|r| r.as_bool()) == Some(true),
            })
        })
        .collect()
}

/// Whether the user allowed `tool` for `chat` and the grant has not run out
pub fn granted(chat: Option<&str>, tool: &str) -> bool {
    chat.is_some_and(|chat| chat_grants(chat).allowed(chat, tool, Instant::now()))
}

/// Turns an approved request into a grant for its chat and tool. Returns (chat, tool, minutes).
pub fn grant_from_request(request_id: &str, minutes: Option<u64>) -> Option<(String, String, u64)> {
    let (chat, tool) = REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).get(request_id).cloned()?;
    let minutes = grants().grant(&chat, &tool, minutes, Instant::now());
    tracing::info!("🔓 [API] '{}' allowed for chat {} for {} minutes", tool, chat, minutes);
    Some((chat, tool, minutes))
}

/// Removes the grants of a chat (all of them, or one tool's), and logs it so a restart does not
/// bring them back
pub fn revoke(chat: &str, tool: Option<&str>) -> usize {
    let removed = chat_grants(chat).revoke(chat, tool);
    if removed > 0 {
        use cluaiz_agents::memory::EventStore;
        let event = cluaiz_agents::memory::NewEvent::new("grant", serde_json::json!({ "tool": tool, "revoked": true, "scope": "chat" }));
        let _ = cluaiz_agents::memory::store_for(chat).append(chat, vec![event]);
    }
    removed
}

/// Drops a chat's grants from RAM without logging (its history was deleted)
pub fn forget(chat: &str) {
    grants().revoke(chat, None);
}

/// The chat's grants that are still running: (tool, minutes left)
pub fn grants_of(chat: &str) -> Vec<(String, u64)> {
    chat_grants(chat).of(chat, Instant::now())
}

/// Whether a call needs approval, and the capabilities shown to the user.
pub struct Assessment {
    pub requires_approval: bool,
    pub capabilities: Vec<String>,
}

/// Engine-internal calls (tool loading) never ask. Registered tools follow their security and
/// execution modes; an unregistered MCP or undeclared tool asks unless the agent has full access.
/// A function of an MCP server whose annotations the user trusts runs without asking when the
/// server marks it read-only (`function`: the MCP function called).
pub fn assess(comp_type: &str, comp_name: &str, function: Option<&str>, engine_internal: bool, chat: Option<&str>) -> Assessment {
    if engine_internal {
        return Assessment { requires_approval: false, capabilities: vec!["tools:load".to_string()] };
    }
    if granted(chat, comp_name) {
        return Assessment { requires_approval: false, capabilities: vec!["granted_for_chat".to_string()] };
    }
    let master_mode = cluaiz_security::PermissionSchema::load().agent_security_mode.to_lowercase();
    let (requires_approval, capabilities) = match cluaiz_agents::ToolsEngine::get_tool(comp_name).ok().flatten() {
        Some(entry) if entry.requires_approval(&master_mode) && read_only_trusted(&entry, &master_mode, function) => {
            (false, vec!["mcp:read_only_trusted".to_string()])
        }
        Some(entry) => (entry.requires_approval(&master_mode), entry.effective_capabilities()),
        None if comp_type == "mcp" => (true, vec!["mcp:external".to_string()]),
        None if master_mode == "full_access" => (false, vec!["full_access_override".to_string()]),
        None => (true, vec!["undeclared".to_string()]),
    };
    Assessment { requires_approval, capabilities }
}

/// The trusted read-only rule for one MCP function, with its annotations from the server's tool
/// list (the cached `tools/list`, as the model saw it)
fn read_only_trusted(entry: &cluaiz_agents::ToolEntry, master_mode: &str, function: Option<&str>) -> bool {
    let Some(function) = function.filter(|_| entry.trust_annotations) else { return false };
    let tools = cluaiz_agents::McpClient::get_cached_tools(std::path::Path::new(&entry.local_dir)).unwrap_or_default();
    let annotations = tools
        .iter()
        .find(|tool| tool.get("name").and_then(serde_json::Value::as_str) == Some(function))
        .and_then(|tool| tool.get("annotations"));
    entry.read_only_trusted(master_mode, annotations)
}

/// A call waiting for the user's decision.
pub struct PendingApproval {
    pub request_id: String,
    decision: oneshot::Receiver<ApprovalDecision>,
}

/// Registers the call for approval. The caller shows it to the user (the streaming path sends
/// a `permission_request` chunk; every path lists it under `/v1/system/permission/pending`).
pub async fn request(state: &Arc<AppState>, comp_type: &str, tool_name: &str, payload: &str, chat: Option<&str>) -> PendingApproval {
    let request_id = format!("perm_{}", uuid::Uuid::new_v4());
    if let Some(chat) = chat {
        REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).insert(request_id.clone(), (chat.to_string(), tool_name.to_string()));
    }
    let (tx, decision) = oneshot::channel::<ApprovalDecision>();
    state.active_approvals.write().await.insert(request_id.clone(), tx);
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    state.pending_permissions.write().await.insert(
        request_id.clone(),
        PendingPermissionRequest {
            id: request_id.clone(),
            action: format!("{}:{}", comp_type, tool_name),
            target: payload.to_string(),
            caller: "agent".to_string(),
            timestamp_ms,
            status: "pending".to_string(),
        },
    );
    PendingApproval { request_id, decision }
}

/// Waits for the decision. `Err` carries the reason the call must not run.
pub async fn wait(state: &Arc<AppState>, pending: PendingApproval, tool_name: &str) -> Result<(), String> {
    let outcome = match tokio::time::timeout(std::time::Duration::from_secs(APPROVAL_TIMEOUT_SECS), pending.decision).await {
        Ok(Ok(decision)) if decision.approved => {
            tracing::info!("✅ [API] User approved execution of tool '{}'", tool_name);
            Ok(())
        }
        Ok(Ok(decision)) => {
            tracing::warn!("🛑 [API] User rejected tool '{}': {:?}", tool_name, decision.feedback);
            Err(decision.feedback.unwrap_or_else(|| "User denied permission to execute this tool.".to_string()))
        }
        Ok(Err(_)) => Err("Approval channel disconnected".to_string()),
        Err(_) => {
            tracing::warn!("⏱️ [API] Confirmation timed out for tool '{}'", tool_name);
            if let Some(req) = state.pending_permissions.write().await.get_mut(&pending.request_id) {
                req.status = "timeout".to_string();
            }
            Err(format!("User confirmation timed out after {} seconds. Operation skipped.", APPROVAL_TIMEOUT_SECS))
        }
    };
    state.active_approvals.write().await.remove(&pending.request_id);
    REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).remove(&pending.request_id);
    outcome
}

/// The user's decision on a tool call, for the chat's event log and the context report
pub fn approval_event(tool_name: &str, decision: &Result<(), String>) -> serde_json::Value {
    serde_json::json!({
        "op": "approval",
        "target": tool_name,
        "ok": decision.is_ok(),
        "reason": decision.as_ref().err(),
    })
}

/// The tool result the model gets when a call was not allowed to run.
pub fn denied_result(reason: &str) -> String {
    serde_json::json!({
        "status": "denied",
        "error": format!("Permission Denied: {}", reason),
        "message": reason
    })
    .to_string()
}
