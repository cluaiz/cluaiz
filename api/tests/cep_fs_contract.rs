//! Real router acceptance for file/workspace CEP methods and retired routes.
mod support;
use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_real_api_router_fs_workspace_round_trip_and_old_routes_removed() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = cluaiz_api::AppState::new_test_state(workspace.path().to_owned());
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::create_router(state);
    let write = support::cep(
        &router,
        "fs.write",
        json!({"path":"notes.md","text":"नमस्ते"}),
    )
    .await?
    .map_err(|error| anyhow!("{error}"))?;
    assert_eq!(write["bytes"], "नमस्ते".len());
    let read = support::cep(&router, "fs.read", json!({"path":"notes.md"}))
        .await?
        .map_err(|error| anyhow!("{error}"))?;
    assert_eq!(read["text"], "नमस्ते");
    let current = support::cep(&router, "workspace.get", json!({}))
        .await?
        .map_err(|error| anyhow!("{error}"))?;
    assert!(std::path::Path::new(current["path"].as_str().unwrap()).is_dir());
    for path in [
        "/v1/fs/read",
        "/v1/fs/write",
        "/v1/fs/list",
        "/v1/fs/delete",
        "/v1/fs/rename",
        "/v1/fs/copy",
        "/v1/fs/mkdir",
        "/v1/fs/pick_folder",
        "/v1/workspace",
    ] {
        // GET cannot open a POST-only native picker if a route regresses.
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header("authorization", "Bearer test_session_token")
            .body(Body::empty())?;
        assert_eq!(
            router.clone().oneshot(request).await?.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    Ok(())
}
