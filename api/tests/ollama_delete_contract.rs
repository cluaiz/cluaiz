//! Ollama's `DELETE /api/delete` on the real router with disposable metadata-only packages
//! (no weights): 200 with an empty body, then 404 for the same model, `name` as the older field,
//! 400 for a bad body, and the same registry result as CEP `models.remove`.
#[path = "../../test/cep/wave_b/isolated_home.rs"]
mod isolated_home;
#[path = "../../engine/models/src/test_fixtures.rs"]
#[allow(dead_code)]
mod model_fixtures;

use anyhow::{anyhow, Result};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistryEntry};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

fn entry(root: &Path, id: &str) -> Result<ModelRegistryEntry> {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("main.gguf");
    model_fixtures::gguf(&path, &[("general.name", id)], &[], Some("test.weight"));
    Ok(serde_json::from_value(json!({
        "id":id,"category":"chat","format_type":"gguf","huggingface_repo":"",
        "local_dir":dir,"files":[{"name":"main.gguf","size_bytes":path.metadata()?.len(),"is_primary":true}],
        "supported_tasks":[],"requires_gpu":false,
        "metadata":{"architecture":"metadata-test","parameters":"unknown","context_window":"unknown",
                    "quantization":null,"bit_depth":null,"chat_template":null}
    }))?)
}

async fn delete(router: &Router, body: &str) -> Result<(StatusCode, String)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/delete")
                .header("authorization", "Bearer test_session_token")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

fn error_of(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or(Value::Null)["error"].clone()
}

#[tokio::test(flavor = "current_thread")]
async fn test_ollama_delete_removes_an_installed_model() -> Result<()> {
    let home = isolated_home::IsolatedHome::new()?;
    let workspace = tempfile::tempdir()?;
    let first = entry(home.path(), "delete-first")?;
    let second = entry(home.path(), "delete-second")?;
    let keep = entry(home.path(), "delete-keep")?;
    let mut registry = cluaiz_models::types::manifest::ModelRegistry::default();
    for e in [&first, &second, &keep] {
        registry.installed_models.insert(e.id.clone(), e.clone());
    }
    InstalledStateRegistry::save(&registry).map_err(|error| anyhow!(error))?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state);

    // Without a token the route is as closed as every other API route
    let denied = router
        .clone()
        .oneshot(Request::builder().method(Method::DELETE).uri("/api/delete").body(Body::from(r#"{"model":"delete-keep"}"#))?)
        .await?;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    assert!(Path::new(&keep.local_dir).exists());

    // Success: 200 and an empty body; folder and registry entry are gone
    let (status, body) = delete(&router, r#"{"model":"delete-first"}"#).await?;
    assert_eq!((status, body.as_str()), (StatusCode::OK, ""));
    assert!(!Path::new(&first.local_dir).exists());
    assert!(InstalledStateRegistry::find(&first.id).is_none());

    // The same model again: 404 with Ollama's error body
    let (status, body) = delete(&router, r#"{"model":"delete-first"}"#).await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error_of(&body), json!("model 'delete-first' not found"));

    // The older field name
    let (status, body) = delete(&router, r#"{"name":"delete-second"}"#).await?;
    assert_eq!((status, body.as_str()), (StatusCode::OK, ""));
    assert!(InstalledStateRegistry::find(&second.id).is_none());

    // Bad requests are 400 and delete nothing
    for body in ["", "not json", "{}", r#"{"model":"  "}"#, r#"{"model":"../escape"}"#, r#"{"model":"a/b"}"#] {
        let (status, text) = delete(&router, body).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}: {text}");
    }
    assert!(Path::new(&keep.local_dir).exists());
    assert!(InstalledStateRegistry::find(&keep.id).is_some());
    Ok(())
}
