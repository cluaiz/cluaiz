//! Real CEP router/dispatcher contracts for telemetry and read-only planning.
mod support;
use anyhow::{anyhow, Result};
use cluaiz_cep::{OnDenied, Principal, Scope};
use serde_json::{json, Value};
use std::sync::Arc;

#[tokio::test]
async fn test_system_telemetry_has_domain_results_and_old_routes_are_gone() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = cluaiz_api::AppState::new_test_state(workspace.path().to_owned());
    let router = cluaiz_api::create_router(state);
    let live = support::cep(&router, "system.live", json!({}))
        .await?
        .map_err(|e| anyhow!("{e}"))?;
    assert_eq!(live["process_id"], std::process::id());
    assert!(live["memory"]["total_gb"].is_number());
    assert!(live["cpu"].is_object() && live["gpus"].is_array());
    assert!(live.get("status").is_none());
    let ps = support::cep(&router, "system.ps", json!({}))
        .await?
        .map_err(|e| anyhow!("{e}"))?;
    assert_eq!(ps["active_processes"], json!([]));
    assert!(ps["hardware_snapshot"].is_object());
    Ok(())
}

#[tokio::test]
async fn test_system_events_reads_by_cursor_and_rejects_bad_params() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = cluaiz_api::AppState::new_test_state(workspace.path().to_owned());
    let router = cluaiz_api::create_router(state);
    // Optional params: an empty object and explicit nulls both answer
    let start = support::cep(&router, "system.events", json!({})).await?.map_err(|e| anyhow!("{e}"))?;
    support::cep(&router, "system.events", json!({"after":null,"limit":null,"wait_ms":null})).await?.map_err(|e| anyhow!("{e}"))?;
    let cursor = start["latest_seq"].as_u64().ok_or_else(|| anyhow!("latest_seq: {start}"))?;
    cluaiz_cep::events::publish("test.contract_load_started", json!({"model":"test-model","attempt":1}));
    let page = support::cep(&router, "system.events", json!({"after":cursor,"limit":256,"wait_ms":0}))
        .await?
        .map_err(|e| anyhow!("{e}"))?;
    let event = page["events"].as_array().and_then(|events| events.iter().find(|e| e["name"] == "test.contract_load_started"))
        .ok_or_else(|| anyhow!("event missing: {page}"))?;
    assert!(event["seq"].as_u64().unwrap_or(0) > cursor && event["at"].is_string() && event["data"]["model"] == "test-model");
    assert!(page["next"].as_u64() >= event["seq"].as_u64() && page["latest_seq"].as_u64() >= page["next"].as_u64());
    for params in [json!({"after":-1}), json!({"limit":"10"}), json!({"wait_ms":1.5}), json!({"since":0})] {
        let error = support::cep(&router, "system.events", params).await?.unwrap_err();
        assert_eq!(error["code"], -32602, "{error}");
    }
    Ok(())
}

#[tokio::test]
async fn test_planner_rejects_bad_params_and_missing_models_without_loading() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_owned(),
    ));
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let missing = format!("missing-{}", uuid::Uuid::new_v4());
    let saved = serde_json::to_value(cluaiz_core::Settings::current())?;
    for params in [
        json!({"model":missing,"batch_tokens":0}),
        json!({"model":missing,"n_ctx":-2}),
        json!({"model":missing,"ram_buffer_gb":-1}),
        json!({"model":missing,"extra":true}),
    ] {
        let error = support::cep(&router, "system.plan", params)
            .await?
            .unwrap_err();
        assert_eq!(error["code"], -32602, "{error}");
    }
    let missing = support::cep(&router, "system.plan", json!({"model":missing}))
        .await?
        .unwrap_err();
    assert_eq!(missing["code"], -32004);
    assert_eq!(missing["data"]["type"], "model_not_found");
    assert_eq!(
        serde_json::to_value(cluaiz_core::Settings::current())?,
        saved
    );
    assert!(state.host.current_plan.lock().await.is_none());
    Ok(())
}

#[tokio::test]
async fn test_plan_path_roots_and_storage_write_scope_are_checked() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let outside = tempfile::NamedTempFile::new()?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_owned(),
    ));
    let principal = Arc::new(Principal {
        id: "token:system-contract".into(),
        scopes: vec![Scope::Read],
        roots: Some(vec![std::fs::canonicalize(workspace.path())?]),
        on_denied: OnDenied::Error,
        owner: false,
    });
    for (method, params, needs) in [
        (
            "system.plan",
            json!({"model":"path-probe","model_path":outside.path()}),
            "roots",
        ),
        ("storage.clean", json!({}), "write"),
    ] {
        let bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))?;
        let reply = state
            .cep
            .call(
                cluaiz_api::cep::CallCtx {
                    state: state.clone(),
                },
                principal.clone(),
                &bytes,
            )
            .await
            .unwrap();
        let reply: Value = serde_json::from_slice(&reply)?;
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
        assert_eq!(reply["error"]["data"]["needs"], needs);
    }
    Ok(())
}
