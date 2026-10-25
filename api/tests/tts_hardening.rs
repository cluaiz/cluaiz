//! Opt-in HTTP/native proof against existing packages, with an isolated home under target/.
#[path = "../../test/cep/wave_b/audio_task.rs"]
mod audio_task;
mod support;
use anyhow::{Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use cluaiz_api::AppState;
use cluaiz_core::{ActiveUserSettings, Settings};
use serde_json::{json, Value};
use std::{num::NonZeroU64, path::PathBuf, sync::Arc};
use tower::ServiceExt;

struct RestoreSettings(ActiveUserSettings);
impl Drop for RestoreSettings {
    fn drop(&mut self) {
        let _ = Settings::update(self.0.clone());
    }
}

fn wav(body: &str) -> Result<Vec<u8>> {
    let value: Value = serde_json::from_str(body)?;
    let encoded = value["output"]["audio_data"]
        .as_str()
        .context("Missing generated WAV")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.split_once(',').context("WAV data URI")?.1)?;
    assert!(bytes.len() > 44 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE");
    assert!(bytes[44..]
        .chunks_exact(2)
        .any(|s| i16::from_le_bytes([s[0], s[1]]).abs_diff(0) > 32));
    Ok(bytes)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires isolated CLUAIZ_HOME under target, installed CLUAIZ_TTS_ENGINE_MODEL and CLUAIZ_TTS_ONNX_MODEL, text/output paths"]
async fn test_real_tts_capabilities_admission_devices_and_backend_replacement() -> Result<()> {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target")
        .canonicalize()?;
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?).canonicalize()?;
    assert!(
        home.starts_with(&target),
        "Never change installed user settings in this test"
    );
    let output = PathBuf::from(std::env::var("CLUAIZ_TTS_OUT_DIR")?).canonicalize()?;
    assert!(output.starts_with(&target));
    let gguf = std::env::var("CLUAIZ_TTS_ENGINE_MODEL")?;
    let onnx = std::env::var("CLUAIZ_TTS_ONNX_MODEL")?;
    let text = std::fs::read_to_string(std::env::var("CLUAIZ_TTS_TEXT_FILE")?)?;
    assert!(!text.trim().is_empty());
    let _restore = RestoreSettings(Settings::current());
    let workspace = tempfile::tempdir_in(&target)?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_owned()));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(Arc::clone(&state));
    let missing = audio_task::run(
        &router,
        json!({"model":"missing-audio-model",
        "input_source":{"type":"text","data":text}}),
    )
    .await?
    .unwrap_err();
    assert_eq!(missing["code"], -32004);
    let catalog = support::cep(&router, "audio.voices", json!({"model":gguf}))
        .await?
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    assert!(catalog["capabilities"]["languages"]
        .as_array()
        .context("Language inventory")?
        .contains(&json!("en")));
    assert_eq!(catalog["capabilities"]["voice_mode"], "reference_source");
    assert_eq!(catalog["capabilities"]["coherent_input"], true);
    assert_eq!(
        catalog["data"].as_array().context("Voice inventory")?.len(),
        1
    );
    assert!(
        state.host.speech.resident_model().is_none(),
        "Discovery must not load weights"
    );
    std::fs::write(
        output.join("voices.json"),
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    if let Ok(models) = std::env::var("CLUAIZ_TTS_DISCOVERY_MODELS") {
        let mut catalogs = Vec::new();
        for model in models.split(';').filter(|m| !m.is_empty()) {
            let catalog = support::cep(&router, "audio.voices", json!({"model":model}))
                .await?
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            assert!(
                catalog["capabilities"]["languages"]
                    .as_array()
                    .is_some_and(|languages| !languages.is_empty()),
                "{catalog}"
            );
            assert!(
                catalog["data"]
                    .as_array()
                    .is_some_and(|voices| !voices.is_empty()),
                "{catalog}"
            );
            catalogs.push(catalog);
        }
        std::fs::write(
            output.join("additional-catalogs.json"),
            serde_json::to_vec_pretty(&catalogs)?,
        )?;
        assert!(
            state.host.onnx.resident_model().await?.is_none(),
            "Discovery must not create ONNX sessions"
        );
    }

    let payload = |model: &str, keep: i32| json!({"model":model,"input":text,"stream":false,"keep_alive":keep});
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/speech")
                .header("content-type", "application/json")
                .body(Body::from(payload(&gguf, 0).to_string()))?,
        )
        .await?;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    let lease = state.host.speech.try_acquire()?;
    for id in [&gguf, &onnx] {
        let (status, body) =
            support::post(&router, "/v1/audio/speech", payload(id, 0), "user").await?;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert!(body.contains("speech_busy"));
    }
    drop(lease);
    let mut unsupported = payload(&gguf, 0);
    unsupported["language"] = json!("hi");
    let (status, body) = support::post(&router, "/v1/audio/speech", unsupported, "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("unsupported_language"));
    let mut corrupt = payload(&gguf, 0);
    corrupt["voice"] = json!("data:audio/wav;base64,AAAA");
    let (status, _) = support::post(&router, "/v1/audio/speech", corrupt, "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(state.host.speech.resident_model().is_none());

    let mut settings = Settings::current();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.n_threads = Some(8);
    settings.optimization.n_threads_batch = Some(8);
    settings.onnx.n_gpu_layers = 0;
    let free = cluaiz_hw::HardwareProber::with_live_memory()
        .memory
        .available_gb;
    settings.media.projector_workspace_bytes =
        NonZeroU64::new(cluaiz_core::gib_to_bytes(free + 1.0)).context("Workspace")?;
    Settings::update(settings.clone())?;
    let (status, body) =
        support::post(&router, "/v1/audio/speech", payload(&gguf, 0), "user").await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("RAM") || body.contains("workspace"), "{body}");
    assert!(state.host.speech.resident_model().is_none());
    settings.media = _restore.0.media;
    Settings::update(settings.clone())?;
    let (result, task_status) = audio_task::run_with_events(
        &router,
        json!({"model":gguf,"input_source":{"type":"text","data":text},"keep_alive":-1}),
    )
    .await?;
    let result = result.map_err(|error| anyhow::anyhow!("{error}"))?;
    assert!(task_status["events"].as_array().is_some_and(|events| events
        .iter()
        .any(|event| event["type"] == "speech.audio.delta")));
    let body = serde_json::to_string(&result)?;
    std::fs::write(output.join("cpu-native.wav"), wav(&body)?)?;
    assert_eq!(
        state.host.speech.resident_model().as_deref(),
        Some(gguf.as_str())
    );
    let reference = serde_json::from_str::<Value>(&body)?["output"]["audio_data"].clone();
    settings.optimization.n_threads = Some(4);
    Settings::update(settings.clone())?;
    let mut conditioned = payload(&gguf, -1);
    conditioned["voice"] = reference;
    let (status, body) = support::post(&router, "/v1/audio/speech", conditioned, "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    std::fs::write(output.join("cpu-reconfigured.wav"), wav(&body)?)?;
    if std::env::var("CLUAIZ_TTS_TEST_GPU").as_deref() == Ok("true") {
        for layers in [None, Some(8)] {
            settings.optimization.n_gpu_layers = layers;
            Settings::update(settings.clone())?;
            let (status, body) =
                support::post(&router, "/v1/audio/speech", payload(&gguf, 0), "user").await?;
            assert_eq!(status, StatusCode::OK, "{body}");
            std::fs::write(output.join(format!("gpu-{layers:?}.wav")), wav(&body)?)?;
            assert!(state.host.speech.resident_model().is_none());
            println!("Real speech HTTP GPU setting {layers:?}: generated PCM and zero-retention release passed");
        }
    }
    let (status, body) =
        support::post(&router, "/v1/audio/speech", payload(&onnx, -1), "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    std::fs::write(output.join("onnx-swap.wav"), wav(&body)?)?;
    assert!(
        state.host.speech.resident_model().is_none(),
        "ONNX selection releases GGUF speech"
    );
    assert_eq!(
        state.host.onnx.resident_model().await?.as_deref(),
        Some(onnx.as_str())
    );
    let (status, body) =
        support::post(&router, "/v1/audio/speech", payload(&gguf, 0), "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    std::fs::write(output.join("native-swap.wav"), wav(&body)?)?;
    assert!(
        state.host.onnx.resident_model().await?.is_none(),
        "GGUF selection releases ONNX TTS"
    );
    assert!(
        state.host.speech.resident_model().is_none(),
        "Zero retention releases native model"
    );
    println!("TTS hardening native HTTP: discovery without weights; 401/429/400; low-RAM rejection/recovery; CPU 8/4 threads; ONNX/GGUF replacement; keep_alive0 passed");
    Ok(())
}
