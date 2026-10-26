//! Direct ingest has HTTP status semantics and returns final results, not CEP tasks.
mod support;

use anyhow::Result;
use axum::http::StatusCode;
use cluaiz_api::AppState;
use serde_json::{json, Value};
use std::sync::Arc;

#[tokio::test]
async fn direct_ingest_returns_text_metadata_and_ordered_partial_results() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state);
    let source = "data:text/plain;base64,SGVsbG8gQ2x1YWl6IQ==";
    let (status, body) = support::post(&router, "/v1/ingest", json!({"source":source})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let reply: Value = serde_json::from_str(&body)?;
    assert_eq!(reply["status"], "success");
    assert_eq!(
        reply["processed_sources"][0]["chunks"],
        json!(["Hello Cluaiz!"])
    );
    assert!(reply.get("task").is_none() && reply.get("result").is_none());

    let (status, body) = support::post(
        &router,
        "/v1/ingest",
        json!({
            "source":source,"output_controls":{"return_text":false}
        }),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let metadata: Value = serde_json::from_str(&body)?;
    assert!(metadata["processed_sources"][0].get("chunks").is_none());
    assert!(metadata["processed_sources"][0]["parts"][0]
        .get("text")
        .is_none());
    assert_eq!(metadata["processed_sources"][0]["chunks_count"], 1);

    let bad = "data:text/plain;base64,AA==";
    let (status, body) =
        support::post(&router, "/v1/ingest", json!({"source":[bad,source]})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let partial: Value = serde_json::from_str(&body)?;
    assert_eq!(partial["status"], "partial");
    assert_eq!(partial["total_sources"], 2);
    assert_eq!(partial["processed_sources"][0]["source"], bad);
    assert_eq!(partial["processed_sources"][0]["status"], "error");
    assert_eq!(partial["processed_sources"][1]["source"], source);
    assert_eq!(
        partial["processed_sources"][1]["chunks"][0],
        "Hello Cluaiz!"
    );
    Ok(())
}

#[tokio::test]
async fn direct_ingest_distinguishes_validation_schema_model_and_parse_errors() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let router = cluaiz_api::routes::build_api_router(Arc::new(AppState::new_test_state(
        workspace.path().to_path_buf(),
    )));
    let source = "data:text/plain;base64,SGVsbG8gQ2x1YWl6IQ==";
    for payload in [
        json!({"source":[]}),
        json!({"source":" "}),
        json!({"source":source,"chunking_strategy":{"overlap":1}}),
        json!({"source":source,"chunking_strategy":{"type":"fixed"}}),
        json!({"source":source,"chunking_strategy":{"max_chunk_size":0}}),
        json!({"source":source,"output_controls":{"return_embeddings":true}}),
        json!({"source":source,"vision_settings":{"max_tokens":1}}),
    ] {
        let (status, body) = support::post(&router, "/v1/ingest", payload).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let error: Value = serde_json::from_str(&body)?;
        assert_eq!(error["error"]["type"], "invalid_request_error");
    }
    for payload in [
        json!({}),
        json!({"source":3}),
        json!({"source":source,"namespace":"unused"}),
    ] {
        let (status, body) = support::post(&router, "/v1/ingest", payload).await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let (status, body) = support::post(
        &router,
        "/v1/ingest",
        json!({"source":source,"model":"missing-ingest-http-fixture"}),
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body)?["error"]["type"],
        "model_not_found"
    );
    let (status, body) = support::post(
        &router,
        "/v1/ingest",
        json!({"source":"data:text/plain;base64,AA=="}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let failed: Value = serde_json::from_str(&body)?;
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["processed_sources"][0]["status"], "error");
    assert!(failed["processed_sources"][0]["error"].is_string());
    Ok(())
}
