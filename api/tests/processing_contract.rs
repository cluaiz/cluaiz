//! Negative HTTP contract: opt-in progress is separate from tokens and preserves authentication.
mod support;

use anyhow::Result;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cluaiz_api::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_processing_unknown_model_reports_terminal_error_without_token_pollution() -> Result<()>
{
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state);
    let request = json!({"model":format!("missing-{}", uuid::Uuid::new_v4()), "stream":true, "return_progress":true,
        "messages":[{"role":"user","content":"invalid model must not run"}]});
    let denied = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let (status, body) =
        support::post(&router, "/v1/chat/completions", request.clone(), "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("event: cluaiz.progress\n"), "{body}");
    let events: Vec<Value> = body
        .lines()
        .filter_map(|s| s.strip_prefix("data: "))
        .filter(|s| *s != "[DONE]")
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let first_id = events[0]["id"].as_str().unwrap();
    assert!(events.iter().all(|e| e["id"].as_str() == Some(first_id)));
    assert_eq!(
        events
            .iter()
            .filter(|e| e["processing"]["stage"] == "request")
            .count(),
        1
    );
    assert!(events.iter().any(|e| e["processing"]["state"] == "failed"));
    assert!(events.iter().any(|e| e.get("error").is_some()));
    assert!(events
        .iter()
        .all(|e| e["choices"][0]["delta"]["content"].is_null()));
    assert!(body.find("\"state\":\"failed\"").unwrap() < body.find("data: [DONE]").unwrap());
    let mut legacy = request;
    legacy["return_progress"] = json!(false);
    let (_, body) = support::post(&router, "/v1/chat/completions", legacy, "user").await?;
    assert!(!body.contains("event: cluaiz.progress"));
    assert!(body.contains("data: [DONE]"));
    Ok(())
}
