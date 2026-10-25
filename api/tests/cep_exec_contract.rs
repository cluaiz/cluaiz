//! `system.exec` (O1) and the input checks of `models.download` / `/api/pull` (O6) through the
//! real router. The exec policy is changed in the test state only, never in a saved permission.json.
mod support;
use anyhow::{anyhow, ensure, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

fn echo() -> Value {
    if cfg!(windows) {
        json!({ "program": "cmd", "args": ["/C", "echo", "cep-exec"] })
    } else {
        json!({ "program": "sh", "args": ["-c", "echo cep-exec"] })
    }
}

#[tokio::test]
async fn test_exec_asks_the_owner_for_every_call_and_refuses_a_shell_line() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = cluaiz_api::AppState::new_test_state(workspace.path().to_owned());
    let router = cluaiz_api::create_router(state.clone());

    // Every call asks; the approval covers that call only
    for _ in 0..2 {
        let asked = support::cep(&router, "system.exec", echo()).await?.err().ok_or_else(|| anyhow!("ran without approval"))?;
        ensure!(asked["code"] == -32003, "{asked}");
        let id = asked["data"]["approval_id"].as_str().ok_or_else(|| anyhow!("no approval id: {asked}"))?.to_string();
        support::cep(&router, "approvals.approve", json!({ "approval_id": id })).await?.map_err(|e| anyhow!("{e}"))?;
        let mut again = echo();
        again["_meta"] = json!({ "approval": id });
        let ran = support::cep(&router, "system.exec", again).await?.map_err(|e| anyhow!("{e}"))?;
        ensure!(ran["exit_code"] == 0 && ran["stdout"].as_str().unwrap_or("").contains("cep-exec"), "{ran}");
    }

    // A shell line is refused until the owner allows it
    let line = json!({ "command": "echo cep-exec" });
    let refused = support::cep(&router, "system.exec", line.clone()).await?.err().ok_or_else(|| anyhow!("shell line ran"))?;
    ensure!(refused["code"] == -32602, "{refused}");
    let both = support::cep(&router, "system.exec", json!({ "program": "x", "command": "y" })).await?.err().ok_or_else(|| anyhow!("accepted both forms"))?;
    ensure!(both["code"] == -32602, "{both}");

    {
        let mut schema = state.permission_schema.write().await;
        schema.exec.allow_shell = true;
        schema.exec.without_approval = true;
    }
    let ran = support::cep(&router, "system.exec", line).await?.map_err(|e| anyhow!("{e}"))?;
    ensure!(ran["stdout"].as_str().unwrap_or("").contains("cep-exec"), "{ran}");
    Ok(())
}

#[tokio::test]
async fn test_download_and_pull_refuse_an_empty_model_before_any_request() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let router = cluaiz_api::create_router(cluaiz_api::AppState::new_test_state(workspace.path().to_owned()));
    let refused = support::cep(&router, "models.download", json!({ "model": "  " })).await?.err().ok_or_else(|| anyhow!("started a task"))?;
    ensure!(refused["code"] == -32602, "{refused}");

    let request = Request::builder()
        .method("POST")
        .uri("/api/pull")
        .header("content-type", "application/json")
        .header("authorization", "Bearer test_session_token")
        .body(Body::from(json!({ "model": "" }).to_string()))?;
    let response = router.oneshot(request).await?;
    ensure!(response.status() == StatusCode::BAD_REQUEST, "{}", response.status());
    let body: Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), usize::MAX).await?)?;
    ensure!(body["error"].is_string(), "{body}");
    Ok(())
}
