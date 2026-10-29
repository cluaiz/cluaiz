//! Request-scoped controls and opt-in real native mid-generation reasoning exit.
mod support;

use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cluaiz_api::{
    handlers::chat::types::{StreamSignals, ACTIVE_STREAMS},
    AppState,
};
use futures::StreamExt;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tower::ServiceExt;

#[tokio::test]
async fn test_skip_control_is_scoped_and_rejects_unknown_unsupported_and_unauthenticated(
) -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state);
    let prefix = uuid::Uuid::new_v4().simple().to_string();
    let id = format!("{prefix}-active");
    let other_id = format!("{prefix}-other");
    let unsupported_id = format!("{prefix}-unsupported");
    let selected = Arc::new(AtomicBool::new(false));
    let other = Arc::new(AtomicBool::new(false));
    {
        let mut streams = ACTIVE_STREAMS.write().unwrap();
        streams.insert(
            id.clone(),
            StreamSignals {
                skip_reasoning: selected.clone(),
                reasoning_supported: true,
                ..Default::default()
            },
        );
        streams.insert(
            other_id.clone(),
            StreamSignals {
                skip_reasoning: other.clone(),
                reasoning_supported: true,
                ..Default::default()
            },
        );
        streams.insert(unsupported_id.clone(), StreamSignals::default());
    }
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cep")
                .header("content-type", "application/json")
                .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"chat.skip_reasoning","params":{"stream_id":id}}).to_string()))?,
        )
        .await?;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert!(!selected.load(Ordering::Acquire));
    for _ in 0..2 {
        let reply = support::cep(&router, "chat.skip_reasoning", json!({"stream_id":id})).await?;
        assert_eq!(reply.expect("skip accepted")["status"], "skip_requested");
    }
    assert!(selected.load(Ordering::Acquire));
    assert!(!other.load(Ordering::Acquire));
    let reply = support::cep(&router, "chat.skip_reasoning", json!({"stream_id":unsupported_id})).await?;
    assert_eq!(reply.unwrap_err()["data"]["type"], "reasoning_unavailable");
    {
        let mut streams = ACTIVE_STREAMS.write().unwrap();
        for key in [&id, &other_id, &unsupported_id] {
            streams.remove(key);
        }
    }
    let reply = support::cep(&router, "chat.skip_reasoning", json!({"stream_id":id})).await?;
    assert_eq!(reply.unwrap_err()["data"]["type"], "stream_not_found");
    Ok(())
}

#[test]
fn test_skip_visibility_preserves_split_native_end_marker_and_answer() {
    let mut filter = cluaiz_models::StreamingReasoningFilter::new(
        Some("<think>".into()),
        Some("</think>".into()),
        false,
        false,
    );
    let mut content = String::new();
    for token in [
        "<thi",
        "nk>private reasoning",
        " still reasoning</thi",
        "nk>FINAL ANSWER",
    ] {
        // Skip hides reasoning output; it must never force the parser into answer mode.
        let (answer, _) = filter.process_token(token);
        if let Some(answer) = answer {
            content.push_str(&answer);
        }
    }
    let (answer, _) = filter.flush_final();
    if let Some(answer) = answer {
        content.push_str(&answer);
    }
    assert_eq!(content, "FINAL ANSWER");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh isolated CLUAIZ_HOME and a read-only thinking GGUF"]
async fn real_skip_reasoning_ends_native_thinking_and_preserves_next_request() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    let isolated_home = std::env::var("CLUAIZ_MEDIA_TEST_HOME")?;
    if home != std::path::Path::new(&isolated_home)
        || home.join("config/model_registry.json").exists()
    {
        bail!("Fresh isolated home required");
    }
    let weights = PathBuf::from(std::env::var("CLUAIZ_REASONING_TEST_MODEL")?);
    let package = tempfile::tempdir()?;
    let primary = package
        .path()
        .join(weights.file_name().context("GGUF name")?);
    std::fs::hard_link(&weights, &primary)?;
    let entry = cluaiz_models::registry::InstalledStateRegistry::entry_for_folder(
        "chat",
        package.path(),
        primary.file_name().and_then(|name| name.to_str()),
        "",
    )
    .context("Thinking model registration")?;
    let id = entry.id.clone();
    let mut registry = cluaiz_models::types::manifest::ModelRegistry::default();
    registry.installed_models.insert(id.clone(), entry);
    cluaiz_models::registry::InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.n_threads = Some(2);
    settings.optimization.n_threads_batch = Some(2);
    settings.optimization.custom_context_window = Some(4096);
    cluaiz_core::Settings::update(settings)?;
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    state.permission_schema.write().await.agent_security_mode = "strict".into();
    state.permission_schema.write().await.stream_telemetry = false;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    for (skip, budget) in [(true, 1024), (false, 1024), (false, 32)] {
        let body = json!({"model":id,"stream":true,"think_mode":"on","reasoning_effort":budget,
            "reasoning_budget_message":"","temperature":0,"max_tokens":if budget == 32 {32} else {192},"keep_alive":-1,
            "messages":[{"role":"user","content":"Calculate 17 times 19. Reply with only the integer, without explanation."}]});
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("authorization", "Bearer test_session_token")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let mut data = response.into_body().into_data_stream();
        let mut pending = String::new();
        let mut stream_id = String::new();
        let mut reasoning = String::new();
        let mut answer = String::new();
        let mut sent = false;
        let mut completed = false;
        while let Some(frame) = tokio::time::timeout(Duration::from_secs(120), data.next()).await? {
            pending.push_str(std::str::from_utf8(&frame?)?);
            while let Some(end) = pending.find("\n\n") {
                let event = pending[..end].to_owned();
                pending.drain(..end + 2);
                for json in event.lines().filter_map(|line| line.strip_prefix("data: ")) {
                    if json == "[DONE]" {
                        completed = true;
                        continue;
                    }
                    let chunk: Value = serde_json::from_str(json)?;
                    assert!(chunk.get("error").is_none(), "{chunk}");
                    if let Some(id) = chunk["id"].as_str() {
                        stream_id = id.to_owned();
                    }
                    if let Some(text) = chunk["choices"][0]["delta"]["reasoning_content"].as_str() {
                        reasoning.push_str(text);
                    }
                    if let Some(text) = chunk["choices"][0]["delta"]["content"].as_str() {
                        answer.push_str(text);
                    }
                }
            }
            if skip && !sent && !stream_id.is_empty() && reasoning.len() >= 12 {
                let reply = support::cep(&router, "chat.skip_reasoning", json!({"stream_id":stream_id})).await?;
                assert_eq!(reply.expect("skip accepted")["status"], "skip_requested");
                sent = true;
            }
        }
        assert!(completed, "Missing stream completion");
        let report = state
            .host
            .last_thinking()
            .await
            .context("Actual native thinking report")?;
        println!(
            "REAL NATIVE SKIP: requested={skip}; sent={sent}; budget={budget}; ended_by={}; reasoning_tokens={}; answer={answer}",
            report.ended_by, report.used_tokens
        );
        if budget == 32 {
            // This case proves automatic native budget enforcement, not answer quality
            // with deliberately interrupted reasoning and a tiny answer allowance.
            assert_eq!(report.ended_by, "budget");
            assert!(report.used_tokens <= 33, "{report:?}");
            assert!(!answer.trim().is_empty());
        } else {
            assert!(
                answer.contains("323"),
                "Real answer after skip={skip}: {answer}"
            );
        }
        assert!(
            !answer.contains("<think") && !answer.contains("</think"),
            "{answer}"
        );
        if skip {
            assert!(sent);
            assert_eq!(report.ended_by, "skip");
        } else {
            assert_ne!(report.ended_by, "skip");
            assert!(!reasoning.is_empty());
        }
        let reply = support::cep(&router, "chat.skip_reasoning", json!({"stream_id":stream_id})).await?;
        assert_eq!(reply.unwrap_err()["data"]["type"], "stream_not_found");
    }
    state.host.unload_model().await;
    Ok(())
}
