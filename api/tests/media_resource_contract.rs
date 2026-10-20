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

#[tokio::test]
async fn test_inline_attachment_transport_accepts_large_json_and_bounds_oversize() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let request = json!({"model":format!("missing-{}", uuid::Uuid::new_v4()),"stream":false,
        "messages":[{"role":"user","content":"x".repeat(3 * cluaiz_core::MIB as usize)}]});
    for route in ["/v1/chat/completions", "/api/chat"] {
        let (status, body) = support::post(&router, route, request.clone(), "user").await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("model_not_found"));
    }
    // Only transport/model lookup is exercised: the absent model must prevent image decoding.
    let embedding = json!({"model":format!("missing-{}", uuid::Uuid::new_v4()),
        "input":[{"type":"image","source":format!("data:image/png;base64,{}",
            "A".repeat(3 * cluaiz_core::MIB as usize))}]});
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test_session_token")
                .body(Body::from(embedding.to_string()))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let error: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(error["error"]["type"], "model_not_found");
    let (status, body) =
        support::post(&router, "/v1/embeddings", embedding.clone(), "user").await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let denied = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .body(Body::from(embedding.to_string()))?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
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
    // Non-attachment JSON endpoints retain their existing default, not the upload ceiling.
    // Use the router directly for this negative: on Windows an early HTTP rejection can
    // reset a still-uploading connection instead of reaching the client's response parser.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/system/plan")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test_session_token")
                .body(Body::from(request.to_string()))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let input = cluaiz_core::Settings::current()
        .media
        .limits
        .max_input_bytes();
    let limit = input
        .div_ceil(3)
        .saturating_mul(4)
        .saturating_add(2 * cluaiz_core::MIB);
    // A declared oversize body rejects before allocation; chunked bodies cannot bypass the cap.
    for (route, declared) in [
        ("/v1/ingest", true),
        ("/v1/ingest", false),
        ("/v1/embeddings", true),
        ("/v1/embeddings", false),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri(route)
            .header("authorization", "Bearer test_session_token")
            .header("origin", "http://localhost:1420")
            .header("content-type", "application/json");
        let body = if declared {
            request = request.header("content-length", limit + 1);
            Body::empty()
        } else {
            let chunk = axum::body::Bytes::from(vec![b' '; 64 * 1024]);
            let count = limit as usize / chunk.len() + 1;
            Body::from_stream(futures::stream::iter(
                (0..count).map(move |_| Ok::<_, std::io::Error>(chunk.clone())),
            ))
        };
        let response = router.clone().oneshot(request.body(body)?).await?;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()["access-control-allow-origin"], "*");
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let error: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(error["error"]["type"], "request_too_large");
    }
    assert!(state.host.current_plan.lock().await.is_none());
    Ok(())
}
