mod support;
use anyhow::{bail, Context, Result};
use axum::http::StatusCode;
use cluaiz_models::{
    models::types::manifest::{ModelRegistry, ModelRegistryEntry},
    registry::InstalledStateRegistry,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

#[test]
fn test_video_transport_is_ordered_and_audio_is_explicit() -> Result<()> {
    use cluaiz_core::media::InputPart;
    let content: cluaiz_api::handlers::chat::MessageContent = serde_json::from_value(json!([
        {"type":"text","text":"before"},
        {"type":"video_url","video_url":{"url":"file:///not-read.mp4"}},
        {"type":"video_url","video_url":{"url":"https://invalid.example/clip.mp4","include_audio":true}},
        {"type":"text","text":"after"}
    ]))?;
    let parts = content.input_parts()?;
    assert!(
        matches!(&parts[1], InputPart::Video { source, include_audio: false } if source == "file:///not-read.mp4")
    );
    assert!(matches!(
        &parts[2],
        InputPart::Video {
            include_audio: true,
            ..
        }
    ));
    assert!(matches!(&parts[3], InputPart::Text { text } if text == "after"));
    assert_eq!(content.text_content(), "before\nafter");
    assert!(
        serde_json::from_value::<cluaiz_api::handlers::chat::MessageContent>(json!([
            {"type":"video_url","video_url":{"url":"clip.mp4","unknown_option":true}}
        ]))
        .is_err()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires real vision GGUF/projector, packaged video provider, and an empty isolated CLUAIZ_HOME"]
async fn real_video_api_json_sse_and_planner_reach_selected_vision_model() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    if home != std::path::Path::new(&std::env::var("CLUAIZ_MEDIA_TEST_HOME")?)
        || home.join("config/model_registry.json").exists()
    {
        bail!("Empty isolated test home required");
    }
    let model = PathBuf::from(std::env::var("CLUAIZ_MEDIA_MODEL")?);
    let parent = model.parent().context("Model parent")?;
    if parent.starts_with(&home) {
        bail!("Read-only model fixture must remain outside test home");
    }
    let geometry = cluaiz_models::GgufProber::probe_geometry(&model)?;
    let model_id = "video-router-fixture";
    let entry: ModelRegistryEntry = serde_json::from_value(json!({
        "id":model_id,"category":"chat","format_type":"gguf","huggingface_repo":"", "local_dir":parent,
        "files":[{"name":model.file_name().context("Filename")?.to_str(),"size_bytes":std::fs::metadata(&model)?.len(),"is_primary":true}],
        "supported_tasks":["chat-completion"],"requires_gpu":false,
        "metadata":{"architecture":geometry.architecture,"parameters":"","context_window":"4096", "quantization":null,"bit_depth":null,"chat_template":null}
    }))?;
    let mut registry = ModelRegistry::default();
    registry.installed_models.insert(model_id.into(), entry);
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.n_threads = Some(2);
    settings.optimization.n_threads_batch = Some(2);
    settings.optimization.custom_context_window = Some(4096);
    settings.optimization.batch_size = Some(128);
    settings.optimization.ubatch_size = Some(128);
    // This isolated small-fixture run retains a 1 GiB host reserve. Keep the
    // production planner/guards intact; do not change the user's saved settings.
    settings.optimization.custom_ram_buffer_gb = Some(1.0);
    settings.media.limits =
        cluaiz_core::media_limits::MediaLimits::new(cluaiz_core::MIB, 16 * cluaiz_core::MIB)?;
    settings.media.projector_workspace_bytes =
        std::num::NonZeroU64::new(512 * cluaiz_core::MIB).context("Workspace")?;
    cluaiz_core::Settings::update(settings)?;
    let permission = cluaiz_security::PermissionSchema {
        agent_security_mode: "strict".into(),
        stream_telemetry: false,
        api_auth: cluaiz_security::ApiAuth {
            required: true,
            ..Default::default()
        },
        ..Default::default()
    };
    permission.save()?;
    let workspace = tempfile::tempdir()?;
    let video_path = workspace.path().join("clip.mp4");
    std::fs::copy(
        PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_FIXTURES")?).join("red-blue-audio.mp4"),
        &video_path,
    )?;
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_path_buf(),
    ));
    *state.permission_schema.write().await = permission;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    for stream in [false, true] {
        let request = json!({"model":model_id,"temperature":0.0,"seed":31,"think_mode":"off","max_tokens":96,"stream":stream,
        "messages":[{"role":"user","content":[
            {"type":"text","text":"State the main color at 0 seconds and at 1 second, in that order. Reply with two color words."},
            {"type":"video_url","video_url":{"url":video_path,"include_audio":false}}
        ]}]});
        let (status, body) =
            support::post(&router, "/v1/chat/completions", request).await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer = if stream {
            let mut answer = String::new();
            for line in body.lines().filter_map(|line| line.strip_prefix("data: ")) {
                if line == "[DONE]" {
                    continue;
                }
                let event: Value = serde_json::from_str(line)?;
                assert!(event.get("error").is_none(), "{event}");
                if let Some(piece) = event["choices"][0]["delta"]["content"].as_str() {
                    answer.push_str(piece);
                }
            }
            assert!(body.contains("data: [DONE]"));
            answer
        } else {
            let response: Value = serde_json::from_str(&body)?;
            response["choices"][0]["message"]["content"]
                .as_str()
                .context("Video answer missing")?
                .to_owned()
        };
        let lower = answer.to_ascii_lowercase();
        let red = lower
            .find("red")
            .context(format!("No red in actual answer: {answer}"))?;
        let blue = lower
            .find("blue")
            .context(format!("No blue in actual answer: {answer}"))?;
        assert!(red < blue, "{answer}");
        println!("REAL VIDEO API: stream={stream}; selected native model answered {answer:?}; red before blue");
    }
    let plan = support::cep(&router, "system.plan", json!({"model":model_id}))
        .await?
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let inputs = plan["model_info"]["inputs"].as_array().context("Inputs")?;
    let image_input = inputs
        .iter()
        .find(|input| input["type"] == "image")
        .context("Image input")?;
    assert_eq!(image_input["available"], true);
    assert!(!inputs.iter().any(|input| input["type"] == "video"));
    let video_input = plan["model_info"]["attachment_processing"]
        .as_array()
        .context("Preparation routes")?
        .iter()
        .find(|route| route["input"] == "video")
        .context("Video preparation")?;
    assert_eq!(video_input["output"], "image");
    assert_eq!(video_input["processing"], "sampled_frames");
    assert!(video_input["formats"]
        .as_array()
        .context("Attachments")?
        .iter()
        .any(|v| v == "mp4"));
    assert!(plan["model_info"].get("supported_attachments").is_none());
    std::fs::write(&video_path, b"corrupt video container")?;
    let (status, body) = support::post(&router, "/v1/chat/completions", json!({"model":model_id,"stream":false,
        "messages":[{"role":"user","content":[{"type":"video_url","video_url":{"url":video_path}}]}]})).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(serde_json::from_str::<Value>(&body)?["error"].is_object());
    state.host.unload_model().await;
    println!("REAL VIDEO PLANNER: loadable package + selected vision pairing advertise mp4; intrinsic input remains image; corrupt source returns error");
    Ok(())
}
