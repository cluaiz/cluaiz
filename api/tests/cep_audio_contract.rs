//! Real CEP router contracts. The tiny package below is metadata, not runnable audio weights.
#[path = "../../test/cep/wave_b/audio_task.rs"]
mod audio_task;
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
};
use cluaiz_cep::{OnDenied, Principal, Scope};
use cluaiz_models::{
    registry::InstalledStateRegistry,
    types::manifest::{ModelRegistry, ModelRegistryEntry},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test(flavor = "current_thread")]
async fn test_audio_validation_tasks_policy_and_route_cutover() -> Result<()> {
    let home = isolated_home::IsolatedHome::new()?;
    let workspace = tempfile::tempdir()?;
    let primary = home.path().join("metadata.gguf");
    model_fixtures::gguf(
        &primary,
        &[("general.name", "metadata")],
        &[],
        Some("test.weight"),
    );
    let entry: ModelRegistryEntry = serde_json::from_value(json!({
        "id":"metadata-speech", "category":"tts", "format_type":"gguf", "huggingface_repo":"",
        "local_dir":home.path(), "files":[{"name":"metadata.gguf","size_bytes":primary.metadata()?.len(),"is_primary":true}],
        "supported_tasks":["text_to_speech"], "requires_gpu":false,
        "metadata":{"architecture":"metadata-test","parameters":"unknown","context_window":"unknown",
            "quantization":null,"bit_depth":null,"chat_template":null}
    }))?;
    let mut registry = ModelRegistry::default();
    registry
        .installed_models
        .insert(entry.id.clone(), entry.clone());
    InstalledStateRegistry::save(&registry).map_err(|e| anyhow!(e))?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_owned(),
    ));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let source = json!({"type":"text","data":"input"});
    for params in [
        json!({"input_source":source,"stream":true}),
        json!({"input_source":{"type":"text","data":"input","extra":true}}),
        json!({"input_source":source,"parameters":{"extra":true}}),
    ] {
        let error = audio_task::run(&router, params).await?.unwrap_err();
        assert_eq!(error["code"], -32602, "{error}");
    }
    for (params, code, kind) in [
        (
            json!({"model":"missing","input_source":source}),
            -32004,
            "model_not_found",
        ),
        (
            json!({"input_source":source,"keep_alive":-2}),
            -32602,
            "unsupported_parameter",
        ),
        (
            json!({"input_source":{"type":"text","data":""}}),
            -32602,
            "invalid_input",
        ),
        (
            json!({"input_source":source,"task":"unknown"}),
            -32602,
            "unsupported_task",
        ),
    ] {
        let (result, view) = audio_task::run_with_events(&router, params).await?;
        let error = result.unwrap_err();
        assert_eq!(error["code"], code, "{error}");
        assert_eq!(error["data"]["type"], kind);
        assert_eq!(view["status"], "failed");
    }
    for method in ["audio.voices", "audio.languages"] {
        assert_eq!(
            support::cep(&router, method, json!({"model":"missing"}))
                .await?
                .unwrap_err()["code"],
            -32004
        );
        assert_eq!(
            support::cep(&router, method, json!({"extra":true}))
                .await?
                .unwrap_err()["code"],
            -32602
        );
    }
    let principal = Arc::new(Principal {
        id: "audio-scoped".into(),
        scopes: vec![Scope::Read],
        roots: Some(vec![workspace.path().canonicalize()?]),
        on_denied: OnDenied::Error,
        owner: false,
    });
    let request = serde_json::to_vec(
        &json!({"jsonrpc":"2.0","id":1,"method":"audio.voices","params":{"model":entry.id}}),
    )?;
    let reply = state
        .cep
        .call(
            cluaiz_api::cep::CallCtx {
                state: state.clone(),
            },
            principal,
            &request,
        )
        .await
        .context("CEP reply")?;
    let reply: Value = serde_json::from_slice(&reply)?;
    assert_eq!(reply["error"]["code"], -32002);
    assert_eq!(reply["error"]["data"]["needs"], "roots");
    // A structurally inspectable graph suffices: source policy must reject before decoding
    // this deliberately invalid WAV or attempting native inference.
    let graph_dir = home.path().join("graph-package");
    std::fs::create_dir(&graph_dir)?;
    let graph = graph_dir.join("main.onnx");
    model_fixtures::onnx(&graph, "test_input", "test_output", None);
    let mut graph_entry = entry.clone();
    graph_entry.id = "metadata-stt".into();
    graph_entry.category = "stt".into();
    graph_entry.format_type = "onnx".into();
    graph_entry.local_dir = graph_dir.to_string_lossy().into_owned();
    graph_entry.files[0].name = "main.onnx".into();
    graph_entry.files[0].size_bytes = graph.metadata()?.len();
    graph_entry.supported_tasks = vec!["speech_to_text".into()];
    graph_entry.metadata.capabilities = Some(cluaiz_models::capabilities::PackageCapabilities::inspect(&graph)?);
    registry.installed_models.insert(graph_entry.id.clone(), graph_entry.clone());
    InstalledStateRegistry::save(&registry).map_err(|e| anyhow!(e))?;
    let outside = tempfile::NamedTempFile::new()?;
    let source_principal = Arc::new(Principal { id:"audio-source-scoped".into(), scopes:vec![Scope::Read],
        roots:Some(vec![graph_dir.canonicalize()?, workspace.path().canonicalize()?]),
        on_denied:OnDenied::Error, owner:false });
    let mut next_params = json!({"model":graph_entry.id,
        "input_source":{"type":"file","data":outside.path()}});
    for method in ["audio.run", "tasks.result"] {
        let bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":next_params}))?;
        let reply = state.cep.call(cluaiz_api::cep::CallCtx { state:state.clone() }, source_principal.clone(), &bytes)
            .await.context("Scoped source reply")?;
        let reply: Value = serde_json::from_slice(&reply)?;
        if method == "audio.run" {
            let id = reply["result"]["task"]["task_id"].as_str().context("Scoped audio task")?;
            next_params = json!({"task_id":id,"wait_ms":5000});
        } else {
            assert_eq!(reply["error"]["code"], -32002, "{reply}");
            assert_eq!(reply["error"]["data"]["needs"], "roots");
        }
    }
    assert!(state.host.speech.resident_model().is_none());
    assert!(state.host.onnx.resident_model().await?.is_none());
    for path in ["/v1/audio/speech", "/v1/audio/transcriptions"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (status, _) = support::post(&router, path, json!({})).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    println!("Audio CEP: strict params, typed failed tasks, model roots, retired routes404 and retained standard auth/shape passed");
    Ok(())
}
