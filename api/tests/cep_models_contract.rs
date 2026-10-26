//! Real router/dispatcher model controls with disposable metadata-only packages.
//! No weights are downloaded or claimed to run inference in these fixtures.
#[path = "../../test/cep/wave_b/isolated_home.rs"]
mod isolated_home;
#[path = "../../engine/models/src/test_fixtures.rs"]
#[allow(dead_code)]
mod model_fixtures;
mod support;

use anyhow::{anyhow, Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use cluaiz_cep::{OnDenied, Principal, Scope};
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistryEntry};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

fn entry(root: &Path, id: &str, category: &str, format: &str) -> Result<ModelRegistryEntry> {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir)?;
    let name = format!("main.{format}");
    let path = dir.join(&name);
    if format == "gguf" {
        model_fixtures::gguf(&path, &[("general.name", id)], &[], Some("test.weight"));
    } else {
        model_fixtures::onnx(&path, "input_ids", "last_hidden_state", None);
    }
    Ok(serde_json::from_value(json!({
        "id":id,"category":category,"format_type":format,"huggingface_repo":"",
        "local_dir":dir,"files":[{"name":name,"size_bytes":path.metadata()?.len(),"is_primary":true}],
        "supported_tasks":[],"requires_gpu":false,
        "metadata":{"architecture":"metadata-test","parameters":"unknown","context_window":"unknown",
                    "quantization":null,"bit_depth":null,"chat_template":null}
    }))?)
}

async fn get(router: &Router, path: &str) -> Result<(StatusCode, Value)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("authorization", "Bearer test_session_token")
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

// One current-thread test owns the environment override and registry for its entire lifetime.
#[tokio::test(flavor = "current_thread")]
async fn test_models_registry_inspect_remove_validation_and_retired_routes() -> Result<()> {
    let home = isolated_home::IsolatedHome::new()?;
    let workspace = tempfile::tempdir()?;
    let gguf = entry(home.path(), "metadata-package", "chat", "gguf")?;
    let onnx = entry(home.path(), "graph-package", "embedding", "onnx")?;
    let mut registry = cluaiz_models::types::manifest::ModelRegistry::default();
    registry
        .installed_models
        .insert(gguf.id.clone(), gguf.clone());
    registry
        .installed_models
        .insert(onnx.id.clone(), onnx.clone());
    InstalledStateRegistry::save(&registry).map_err(|error| anyhow!(error))?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_owned(),
    ));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let listed = support::cep(&router, "models.list", json!({}))
        .await?
        .map_err(|e| anyhow!(e))?;
    let listed = listed.as_array().context("Domain result is an array")?;
    assert_eq!(listed.len(), 2);
    let actual = listed
        .iter()
        .find(|e| e["id"] == gguf.id)
        .context("GGUF entry")?;
    assert_eq!(actual["format"], "gguf");
    assert_eq!(actual["size"], gguf.size_bytes());
    assert!(actual["metadata"].is_object());
    assert!(actual["experimental"].is_boolean());
    assert!(actual.get("status").is_none() && actual.get("format_type").is_none());
    let filtered = support::cep(&router, "models.list", json!({"category":"embedding"}))
        .await?
        .map_err(|e| anyhow!(e))?;
    assert_eq!(filtered.as_array().context("Filtered entries")?.len(), 1);
    assert_eq!(filtered[0]["id"], onnx.id);
    for path in ["/v1/models", "/api/tags"] {
        let (status, value) = get(&router, path).await?;
        assert_eq!(status, StatusCode::OK);
        let entries = value[if path == "/v1/models" {
            "data"
        } else {
            "models"
        }]
        .as_array()
        .context("Standard model list")?;
        assert_eq!(entries.len(), listed.len());
    }
    for params in [
        json!({"model":gguf.id}),
        json!({"model":gguf.id,"file":"main.gguf"}),
    ] {
        let inspected = support::cep(&router, "models.inspect", params)
            .await?
            .map_err(|e| anyhow!(e))?;
        assert_eq!(inspected["metadata"]["general.name"], gguf.id);
        assert_eq!(inspected["tensors"][0]["name"], "test.weight");
    }
    for model in ["../escape", "a/b", "a\\b", "a\0b", "  "] {
        for method in ["models.inspect", "models.load", "models.remove"] {
            let error = support::cep(&router, method, json!({"model":model}))
                .await?
                .unwrap_err();
            assert_eq!(error["code"], -32602, "{method}: {error}");
        }
    }
    for params in [json!({"category":"unknown"}), json!({"extra":true})] {
        assert_eq!(
            support::cep(&router, "models.list", params)
                .await?
                .unwrap_err()["code"],
            -32602
        );
    }
    for file in ["../main.gguf", "", "bad\0name"] {
        assert_eq!(
            support::cep(
                &router,
                "models.inspect",
                json!({"model":gguf.id,"file":file})
            )
            .await?
            .unwrap_err()["code"],
            -32602
        );
    }
    let external = tempfile::NamedTempFile::new()?;
    let link = Path::new(&gguf.local_dir).join("outside.gguf");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(external.path(), &link)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(external.path(), &link)?;
    let error = support::cep(
        &router,
        "models.inspect",
        json!({"model":gguf.id,"file":"outside.gguf"}),
    )
    .await?
    .unwrap_err();
    assert_eq!(error["code"], -32602);
    assert!(external.path().exists());
    for method in ["models.inspect", "models.load", "models.remove"] {
        let error = support::cep(&router, method, json!({"model":"missing-package"}))
            .await?
            .unwrap_err();
        assert_eq!(error["code"], -32004);
        assert_eq!(error["data"]["type"], "model_not_found");
    }
    // A download takes a Hugging Face repo (`owner/name`) too; only an empty or control-character id is refused here
    for model in ["a\0b", "  "] {
        let error = support::cep(&router, "models.download", json!({"model":model}))
            .await?
            .unwrap_err();
        assert_eq!(error["code"], -32602, "{error}");
    }
    let error = support::cep(&router, "models.load", json!({"model":onnx.id}))
        .await?
        .unwrap_err();
    assert_eq!(error["code"], -32007);
    assert!(state.host.current_plan.lock().await.is_none());
    let principal = Arc::new(Principal {
        id: "token:model-test".into(),
        scopes: vec![Scope::Read, Scope::Write],
        roots: Some(vec![workspace.path().canonicalize()?]),
        on_denied: OnDenied::Error,
        owner: false,
    });
    for method in ["models.inspect", "models.load", "models.remove"] {
        let request = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":1,"method":method,"params":{"model":gguf.id}}),
        )?;
        let reply = state
            .cep
            .call(
                cluaiz_api::cep::CallCtx {
                    state: state.clone(),
                },
                principal.clone(),
                &request,
            )
            .await
            .context("Reply")?;
        let reply: Value = serde_json::from_slice(&reply)?;
        assert_eq!(reply["error"]["code"], -32002);
        assert_eq!(reply["error"]["data"]["needs"], "roots");
        assert!(Path::new(&gguf.local_dir).exists());
    }
    let removed = support::cep(&router, "models.remove", json!({"model":gguf.id}))
        .await?
        .map_err(|e| anyhow!(e))?;
    assert_eq!(removed["model"], gguf.id);
    assert!(!Path::new(&gguf.local_dir).exists());
    assert!(InstalledStateRegistry::find(&gguf.id).is_none());
    assert!(Path::new(&onnx.local_dir).exists());
    Ok(())
}
