//! Admission precedes model load/source fetch; plain text does not consume media admission.
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
async fn test_busy_attachment_rejects_before_fetch_and_releases_for_retry() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let lease = state.host.media_admission.try_acquire()?;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    // The nonexistent model and unreadable source must never be reached while admission is held.
    let model = format!("missing-{}", uuid::Uuid::new_v4());
    let mut request = json!({"model":model,"stream":false,"messages":[{"role":"user","content":[
        {"type":"image_url","image_url":{"url":"http://127.0.0.1:1/must-not-be-fetched"}}
    ]}]});
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
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let body: Value = serde_json::from_str(&body)?;
    assert_eq!(body["error"]["code"], "media_busy");
    request["stream"] = json!(true);
    request["return_progress"] = json!(true);
    let (status, body) =
        support::post(&router, "/v1/chat/completions", request.clone(), "user").await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"code\":\"media_busy\""), "{body}");
    assert!(body.contains("\"state\":\"failed\""), "{body}");
    assert_eq!(body.matches("data: [DONE]").count(), 1, "{body}");

    let plain = json!({"model":model,"stream":false,"messages":[{"role":"user","content":"text"}]});
    let (_, body) = support::post(&router, "/v1/chat/completions", plain, "user").await?;
    assert!(
        body.contains("model_not_found") && !body.contains("media_busy"),
        "{body}"
    );
    drop(lease);
    request["stream"] = json!(false);
    let (_, body) = support::post(&router, "/v1/chat/completions", request, "user").await?;
    assert!(
        body.contains("model_not_found") && !body.contains("media_busy"),
        "{body}"
    );
    // A failed preflight released its own permit rather than poisoning subsequent requests.
    assert!(state.host.media_admission.try_acquire().is_ok());
    assert!(state.host.current_plan.lock().await.is_none());
    Ok(())
}
