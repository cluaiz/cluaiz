//! Live chat usage contract for a native GGUF with MTP support.

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use cluaiz_api::AppState;
use cluaiz_core::{ActiveUserSettings, FeatureState, Settings};
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistry};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

#[tokio::test]
#[ignore = "requires a fresh isolated home and CLUAIZ_MTP_TEST_MODEL pointing to a native MTP GGUF"]
async fn mtp_usage_is_returned_for_streaming_and_non_streaming() -> Result<()> {
    let home = PathBuf::from(std::env::var_os("CLUAIZ_HOME").context("Set CLUAIZ_HOME")?);
    let expected = PathBuf::from(
        std::env::var_os("CLUAIZ_MTP_TEST_HOME").context("Set CLUAIZ_MTP_TEST_HOME")?,
    );
    anyhow::ensure!(
        home.is_absolute() && home == expected && home.read_dir()?.next().is_none(),
        "Use a fresh empty isolated home for this test"
    );

    let model = PathBuf::from(
        std::env::var_os("CLUAIZ_MTP_TEST_MODEL").context("Set CLUAIZ_MTP_TEST_MODEL")?,
    );
    let package = tempfile::tempdir_in(&home)?;
    let name = model
        .file_name()
        .and_then(|name| name.to_str())
        .context("GGUF filename")?;
    std::fs::hard_link(&model, package.path().join(name))?;
    let entry = InstalledStateRegistry::entry_for_folder("chat", package.path(), Some(name), "")
        .context("Register the MTP model")?;
    let model_id = entry.id.clone();
    let mut registry = ModelRegistry::default();
    registry.installed_models.insert(model_id.clone(), entry);
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;

    let mut settings = ActiveUserSettings::default();
    settings.optimization.mtp = FeatureState::On;
    Settings::update(settings)?;
    let workspace = tempfile::tempdir_in(&home)?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    let mut permissions = cluaiz_security::PermissionSchema::load();
    permissions.stream_telemetry = false;
    permissions.save()?;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    for stream in [false, true] {
        let mut body = json!({
            "model": model_id,
            "stream": stream,
            "temperature": 0,
            "think_mode": "off",
            "max_tokens": 96,
            "messages": [{
                "role": "user",
                "content": "Return a JSON array containing all integers from 1 through 100 in order."
            }]
        });
        if stream {
            body["stream_options"] = json!({ "include_usage": true });
        }
        let response = router
            .clone()
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("authorization", "Bearer test_session_token")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))?,
            )
            .await?;
        let status = response.status();
        let response = to_bytes(response.into_body(), 2 * 1024 * 1024).await?;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&response)
        );
        let usage = if stream {
            std::str::from_utf8(&response)?
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .filter(|line| *line != "[DONE]")
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .find_map(|mut chunk| {
                    chunk
                        .get_mut("usage")
                        .filter(|value| value.is_object())
                        .map(Value::take)
                })
                .context("Final SSE usage event")?
        } else {
            serde_json::from_slice::<Value>(&response)?["usage"].clone()
        };
        let drafted = usage["mtp"]["drafted"].as_u64().context("Draft count")?;
        let accepted = usage["mtp"]["accepted"]
            .as_u64()
            .context("Acceptance count")?;
        assert!(drafted > 0 && accepted <= drafted, "{usage}");
        assert_eq!(usage["mtp"]["stopped"], false);
        assert!(usage.get("context_telemetry").is_none());
    }
    state.host.unload_model().await;
    Ok(())
}
