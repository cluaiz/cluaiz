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

/// A symbolic link, or `None` when this machine does not let the test create one
fn link(target: &std::path::Path, at: &std::path::Path) -> Option<()> {
    #[cfg(windows)]
    let made = if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, at)
    } else {
        std::os::windows::fs::symlink_file(target, at)
    };
    #[cfg(not(windows))]
    let made = std::os::unix::fs::symlink(target, at);
    made.map_err(|error| eprintln!("skipped: cannot create a symbolic link here ({error})")).ok()
}

#[tokio::test]
async fn test_delete_and_rename_act_on_a_link_not_its_target() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let kept = outside.path().join("keep.txt");
    std::fs::write(&kept, "keep")?;
    if link(outside.path(), &workspace.path().join("folder-link")).is_none()
        || link(&kept, &workspace.path().join("file-link")).is_none()
    {
        return Ok(());
    }
    let router = cluaiz_api::create_router(cluaiz_api::AppState::new_test_state(workspace.path().to_owned()));

    support::cep(&router, "fs.delete", json!({"path": "folder-link"})).await?.map_err(|e| anyhow!("{e}"))?;
    assert!(std::fs::symlink_metadata(workspace.path().join("folder-link")).is_err(), "the link is gone");
    assert!(kept.is_file(), "the folder it pointed to is untouched");

    support::cep(&router, "fs.rename", json!({"from": "file-link", "to": "renamed-link"})).await?.map_err(|e| anyhow!("{e}"))?;
    let renamed = std::fs::symlink_metadata(workspace.path().join("renamed-link"))?;
    assert!(renamed.file_type().is_symlink(), "the link itself was renamed");
    assert!(kept.is_file(), "the file it points to stays where it was");

    let refused = support::cep(&router, "fs.delete", json!({"path": "folder/.."})).await?.err().ok_or_else(|| anyhow!("deleted `..`"))?;
    assert_eq!(refused["code"], -32602);
    Ok(())
}
