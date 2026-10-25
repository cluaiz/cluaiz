mod support;
use support::post;
fn plan_input<'a>(
    plan: &'a serde_json::Value,
    modality: &str,
) -> anyhow::Result<&'a serde_json::Value> {
    plan["model_info"]["inputs"]
        .as_array()
        .and_then(|inputs| inputs.iter().find(|input| input["type"] == modality))
        .ok_or_else(|| anyhow::anyhow!("Planner input {modality} missing"))
}
// Real router/native acceptance. External model/audio paths and an isolated CLUAIZ_HOME are required.
use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use cluaiz_api::{
    handlers::chat::{
        preflight::{media_continuation, PreparedChatContext},
        ExternalChatRequest,
    },
    AppState,
};
use cluaiz_core::{
    media::{DecodedImage, PreparedPart},
    ChatEnvelope, ChatMessage,
};
use cluaiz_models::{
    models::types::manifest::{ModelRegistry, ModelRegistryEntry},
    registry::InstalledStateRegistry,
};
use cluaiz_runtime::onnx_host::{AudioInput, AudioTask, OnnxModel};
use image::{DynamicImage, ImageOutputFormat, Rgb, RgbImage};
use serde_json::{json, Value};
use std::{io::Cursor, path::PathBuf, sync::Arc};
use tower::ServiceExt;

fn envelope(parts: Vec<PreparedPart>) -> ChatEnvelope<Vec<PreparedPart>> {
    ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: parts,
        }],
        samplers: cluaiz_core::SamplerDefaults {
            temp: Some(0.0),
            seed: Some(31),
            ..Default::default()
        },
        think_mode: "off".into(),
        reasoning_effort: Default::default(),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
    }
}

#[test]
fn continuation_retains_real_buffers_and_request_options() -> Result<()> {
    let image = DecodedImage::new(1, 1, Arc::from([255, 0, 0]))?;
    let pixels = image.pixels().as_ptr();
    let original = envelope(vec![PreparedPart::Image { image }]);
    let continued = media_continuation(
        &original,
        Some(
            r#"[PIVOT_CONTINUE]{"pivot_prompt":"Tool returned a result; inspect the original image."}"#,
        ),
    )?;
    assert_eq!(original.messages[0].content.len(), 1);
    assert_eq!(continued.messages[0].content.len(), 2);
    assert_eq!(continued.samplers, original.samplers);
    drop(original);
    let PreparedPart::Image { image } = &continued.messages[0].content[0] else {
        bail!("Lost original image")
    };
    assert_eq!(image.pixels().as_ptr(), pixels);
    assert_eq!(image.pixels(), [255, 0, 0]);
    assert!(media_continuation(&continued, Some("invalid JSON")).is_err());
    assert!(media_continuation(&continued, Some("{} ")).is_err());
    Ok(())
}

fn png() -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::from_pixel(128, 128, Rgb([255, 0, 0])))
        .write_to(&mut bytes, ImageOutputFormat::Png)?;
    Ok(bytes.into_inner())
}

#[test]
fn transport_parts_preserve_order_without_reading_sources() -> Result<()> {
    use cluaiz_api::handlers::chat::MessageContent;
    use cluaiz_core::media::InputPart;
    let content: MessageContent = serde_json::from_value(json!([
        {"type":"text","text":"before"},
        {"type":"image_url","image_url":{"url":"file:///must-not-be-read.png"}},
        {"type":"input_audio","input_audio":{"format":"wav","data":"AAAA"}},
        {"type":"text","text":"after"}
    ]))?;
    assert_eq!(content.text_content(), "before\nafter");
    let parts = content.input_parts()?;
    assert!(matches!(&parts[0],InputPart::Text { text } if text == "before"));
    assert!(
        matches!(&parts[1],InputPart::Image { source } if source == "file:///must-not-be-read.png")
    );
    assert!(
        matches!(&parts[2],InputPart::Audio { source } if source.starts_with("data:audio/") && source.ends_with(";base64,AAAA"))
    );
    assert!(matches!(&parts[3],InputPart::Text { text } if text == "after"));
    let invalid: MessageContent = serde_json::from_value(json!([
        {"type":"input_audio","input_audio":{"format":"invalid/format","data":"AAAA"}}
    ]))?;
    assert!(invalid.input_parts().is_err());
    assert!(serde_json::from_str::<cluaiz_core::media::MediaSettings>(
        r#"{"projector_workspace_bytes":0}"#
    )
    .is_err());
    Ok(())
}

fn chat(model: &str, content: Value, stream: bool) -> Value {
    json!({"model":model, "messages":[{"role":"user","content":content}],
        "temperature":0.0,"seed":31,"think_mode":"off","max_tokens":96,"stream":stream})
}

async fn collect_native(response: cluaiz_runtime::HostResponse) -> Result<String> {
    match response {
        cluaiz_runtime::HostResponse::TokenStream(mut receiver) => {
            let mut answer = String::new();
            while let Some(piece) = receiver.recv().await {
                if let Some(error) = cluaiz_runtime::stream_error(&piece) {
                    bail!("{error}");
                }
                answer.push_str(&piece);
            }
            Ok(answer)
        }
        cluaiz_runtime::HostResponse::Completed(answer) => Ok(answer),
        cluaiz_runtime::HostResponse::Error(error) => bail!("{error}"),
    }
}

#[tokio::test]
#[ignore = "requires real installed model/audio and isolated CLUAIZ_HOME / CLUAIZ_MEDIA_TEST_HOME"]
async fn real_router_image_audio_metadata_errors_and_continuation() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    let test_home = PathBuf::from(std::env::var("CLUAIZ_MEDIA_TEST_HOME")?);
    if home != test_home || home.join("engine/config/model_registry.json").exists() {
        bail!("An empty isolated media test home is required");
    }
    let model_path = PathBuf::from(std::env::var("CLUAIZ_MEDIA_MODEL")?);
    let model_dir = model_path.parent().context("Model has no parent")?;
    let model_name = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Model filename must be UTF-8")?;
    if model_dir.starts_with(&home) {
        bail!("Model assets must remain outside the mutable test home");
    }
    let audio_url = std::env::var("CLUAIZ_MEDIA_AUDIO_URL")?;
    let expected = std::env::var("CLUAIZ_MEDIA_EXPECTED")?.to_ascii_lowercase();
    if expected.trim().is_empty() {
        bail!("Audio ground truth is required");
    }
    let geometry = cluaiz_models::GgufProber::probe_geometry(&model_path)?;
    let model_id = "media-router-fixture";
    let entry: ModelRegistryEntry = serde_json::from_value(json!({
        "id":model_id,"category":"chat","format_type":"gguf","huggingface_repo":"",
        "local_dir":model_dir,"files":[{"name":model_name,"size_bytes":std::fs::metadata(&model_path)?.len(),"is_primary":true}],
        "supported_tasks":["chat-completion"],"requires_gpu":false,
        "metadata":{"architecture":geometry.architecture,"parameters":"","context_window":"4096",
            "quantization":null,"bit_depth":null,"chat_template":null}
    }))?;
    let mut registry = ModelRegistry::default();
    registry.installed_models.insert(model_id.into(), entry);
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.n_threads = Some(2);
    settings.optimization.n_threads_batch = Some(2);
    settings.optimization.batch_size = Some(128);
    settings.optimization.ubatch_size = Some(128);
    settings.optimization.custom_context_window = Some(4096);
    // Small fixtures only: measured native compute buffers were 32.04 MiB for image
    // and 11.51 MiB for audio. This is not a total-process or universal peak claim.
    settings.media.projector_workspace_bytes =
        std::num::NonZeroU64::new(512 * cluaiz_core::MIB).context("Test workspace")?;
    settings.media.limits = cluaiz_core::media_limits::MediaLimits::new(
        settings.media.limits.max_input_bytes(),
        16 * cluaiz_core::MIB,
    )?;
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
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    *state.permission_schema.write().await = permission;
    let router = cluaiz_api::routes::build_api_router(Arc::clone(&state));
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    chat(model_id, json!("hello"), false).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let plan_request = json!({"model":model_id});
    let (_, before) = post(&router, "/v1/system/plan", plan_request.clone(), "user").await?;
    let before: Value = serde_json::from_str(&before)?;
    for modality in ["text", "image", "audio"] {
        assert_eq!(plan_input(&before, modality)?["type"], modality);
        assert_eq!(plan_input(&before, modality)?["available"], true);
    }
    let current_settings = cluaiz_core::Settings::current();
    let (first, second) = tokio::join!(
        state
            .host
            .ensure_loaded(model_id, &model_path, &current_settings),
        state
            .host
            .ensure_loaded(model_id, &model_path, &current_settings),
    );
    assert_eq!(first?.context_window, second?.context_window);
    let image_bytes = png()?;
    let image_content = json!([
        {"type":"text","text":"What is the main color of this image? Reply with one color word only."},
        {"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&image_bytes))}}
    ]);
    let image_request = chat(model_id, image_content.clone(), false);
    let (status, image) = post(
        &router,
        "/v1/chat/completions",
        image_request.clone(),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{image}");
    let image: Value = serde_json::from_str(&image)?;
    let answer = image["choices"][0]["message"]["content"]
        .as_str()
        .context("Image answer missing")?;
    eprintln!("REAL API JSON image answer: {answer}");
    assert_eq!(answer.trim().to_ascii_lowercase(), "red");

    let audio_content = json!([
        {"type":"text","text":"Transcribe the spoken words in this audio. Reply only with the transcript."},
        {"type":"audio_url","audio_url":{"url":audio_url}}
    ]);
    let (status, events) = post(
        &router,
        "/v1/chat/completions",
        chat(model_id, audio_content, true),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let mut transcript = String::new();
    for line in events
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
    {
        if line == "[DONE]" {
            continue;
        }
        let event: Value = serde_json::from_str(line)?;
        assert!(event.get("error").is_none(), "{event}");
        if let Some(piece) = event["choices"][0]["delta"]["content"].as_str() {
            transcript.push_str(piece);
        }
    }
    eprintln!("REAL API SSE audio answer: {transcript}");
    assert!(transcript.to_ascii_lowercase().contains(&expected));
    assert!(events.contains("data: [DONE]"));
    let wav = reqwest::get(&audio_url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let mixed = json!([
        {"type":"text","text":"Name the main color of the image and transcribe the spoken words in the audio."},
        image_content[1].clone(),
        {"type":"input_audio","input_audio":{"format":"wav","data":base64::engine::general_purpose::STANDARD.encode(wav)}}
    ]);
    let (status, mixed) = post(
        &router,
        "/v1/chat/completions",
        chat(model_id, mixed, false),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{mixed}");
    let mixed: Value = serde_json::from_str(&mixed)?;
    let mixed_answer = mixed["choices"][0]["message"]["content"]
        .as_str()
        .context("Mixed answer missing")?;
    eprintln!("REAL API image + inline audio answer: {mixed_answer}");
    assert!(mixed_answer.to_ascii_lowercase().contains("red"));
    assert!(mixed_answer.to_ascii_lowercase().contains(&expected));
    let (_, after) = post(&router, "/v1/system/plan", plan_request, "user").await?;
    let after: Value = serde_json::from_str(&after)?;
    assert_eq!(
        before["model_info"]["inputs"],
        after["model_info"]["inputs"]
    );
    assert!(plan_input(&after, "image")?["formats"]
        .as_array()
        .context("Image formats")?
        .iter()
        .any(|format| format == "png"));
    assert!(plan_input(&after, "audio")?["formats"]
        .as_array()
        .context("Audio formats")?
        .iter()
        .any(|format| format == "wav"));
    assert!(plan_input(&after, "text")?.get("formats").is_none());
    assert!(after["model_info"].get("supported_attachments").is_none());

    let request: ExternalChatRequest = serde_json::from_value(image_request.clone())?;
    let headers = axum::http::HeaderMap::new();
    let ctx =
        PreparedChatContext::prepare(&state, &request, "continuation-fixture", &headers, None)
            .await
            .map_err(|response| {
                anyhow::anyhow!("Media preparation failed: {}", response.status())
            })?;
    let continued = collect_native(ctx.dispatch(&state.host,Some(r#"[PIVOT_CONTINUE]{"pivot_prompt":"The tool returned no extra visual data. Inspect the original image and reply with its main color, one word only."}"#),32).await).await?;
    eprintln!("REAL media continuation answer: {continued}");
    assert_eq!(continued.trim().to_ascii_lowercase(), "red");
    {
        use cluaiz_api::handlers::chat::types::{StreamSignals, ACTIVE_STREAMS};
        ACTIVE_STREAMS
            .write()
            .expect("test stream registry")
            .insert(
                ctx.request_id.clone(),
                StreamSignals {
                    cancel: Arc::clone(&ctx.cancel_signal),
                    ..Default::default()
                },
            );
        let cancelled = support::cep(&router, "chat.cancel", json!({"stream_id":ctx.request_id})).await?;
        assert_eq!(cancelled.expect("cancel accepted")["status"], "cancelled");
        let stopped = collect_native(ctx.dispatch(&state.host, None, 32).await).await;
        assert!(stopped
            .unwrap_err()
            .to_string()
            .contains("cancelled before execution"));
        ACTIVE_STREAMS
            .write()
            .expect("test stream registry")
            .remove(&ctx.request_id);
        assert!(cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(std::sync::atomic::Ordering::Acquire));
    }
    let prepared = ctx
        .prepared_media
        .as_ref()
        .context("Prepared image missing")?
        .clone();
    let mismatch = collect_native(
        state
            .host
            .dispatch_prepared(
                prepared,
                32,
                &home.join("different-model.gguf"),
                &ctx.request_settings,
                None,
                None,
            )
            .await,
    )
    .await;
    assert!(mismatch
        .unwrap_err()
        .to_string()
        .contains("Model/configuration changed"));

    let outside = tempfile::tempdir()?;
    let external = outside.path().join("outside.png");
    std::fs::write(&external, &image_bytes)?;
    let (status, external_reply) = post(
        &router,
        "/v1/chat/completions",
        chat(
            model_id,
            json!([
                {"type":"image_url","image_url":{"url":external}}
            ]),
            false,
        ),
        "agent",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{external_reply}");
    std::fs::write(workspace.path().join("inside.png"), &image_bytes)?;
    let inside: ExternalChatRequest = serde_json::from_value(chat(
        model_id,
        json!([
            {"type":"image_url","image_url":{"url":"inside.png"}}
        ]),
        false,
    ))?;
    let mut agent_headers = axum::http::HeaderMap::new();
    agent_headers.insert("x-caller", "agent".parse()?);
    let inside_ctx = PreparedChatContext::prepare(
        &state,
        &inside,
        "owner-inside-fixture",
        &agent_headers,
        None,
    )
    .await
    .map_err(|response| {
        anyhow::anyhow!("Owner workspace source failed: {}", response.status())
    })?;
    assert!(inside_ctx.prepared_media.is_some());
    let (status, missing) = post(
        &router,
        "/v1/chat/completions",
        chat("missing-model-fixture", image_content, false),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_str::<Value>(&missing)?["error"]["code"],
        "model_not_found"
    );
    let mut oversized = image_request;
    oversized["max_tokens"] = json!(4096);
    let (status, failed) = post(&router, "/v1/chat/completions", oversized.clone(), "user").await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failed}");
    assert!(serde_json::from_str::<Value>(&failed)?
        .get("error")
        .is_some());
    oversized["stream"] = json!(true);
    let (_, failed_events) = post(&router, "/v1/chat/completions", oversized, "user").await?;
    assert!(failed_events
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| *line != "[DONE]")
        .any(|line| serde_json::from_str::<Value>(line)
            .is_ok_and(|event| event.get("error").is_some())));
    let (status, recovery) = post(
        &router,
        "/api/chat",
        chat(
            model_id,
            json!("What is 2 plus 3? Reply with the digit only."),
            false,
        ),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let recovery: Value = serde_json::from_str(&recovery)?;
    assert!(recovery["choices"][0]["message"]["content"]
        .as_str()
        .context("Text recovery missing")?
        .contains('5'));
    cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, std::sync::atomic::Ordering::Release);
    let legacy = json!({"messages":[{"role":"user","content":"What is 2 plus 3? Reply with the digit only."}],"think_mode":"off","samplers":{"temp":0.0,"seed":31}}).to_string();
    assert!(
        collect_native(state.host.dispatch_stream(&legacy, 32).await)
            .await?
            .contains('5')
    );
    state.host.unload_model().await;
    assert!(state.host.model_description().await.is_none());
    eprintln!("REAL API metadata, auth, workspace deny/allow, model identity, explicit errors, text recovery and unload assertions passed");
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed ONNX speech packages, online WAV and isolated CLUAIZ_HOME"]
async fn real_router_onnx_audio_selected_model_and_retention() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    let test_home = PathBuf::from(std::env::var("CLUAIZ_MEDIA_TEST_HOME")?);
    if home != test_home || home.join("engine/config/model_registry.json").exists() {
        bail!("An empty isolated media test home is required");
    }
    let tts = PathBuf::from(std::env::var("CLUAIZ_ONNX_TTS_MODEL")?);
    let stt = PathBuf::from(std::env::var("CLUAIZ_ONNX_STT_MODEL")?);
    let mut registry = ModelRegistry::default();
    let mut ids = Vec::new();
    for (category, model) in [("tts", &tts), ("stt", &stt)] {
        let dir = model.parent().context("Speech model parent")?;
        if dir.starts_with(&home) {
            bail!("Speech weights must remain outside test home");
        }
        let entry = InstalledStateRegistry::entry_for_folder(
            category,
            dir,
            Some(
                model
                    .file_name()
                    .and_then(|s| s.to_str())
                    .context("Speech filename")?,
            ),
            "",
        )
        .context("Speech registry entry")?;
        ids.push(entry.id.clone());
        registry.installed_models.insert(entry.id.clone(), entry);
    }
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.onnx.n_gpu_layers = 0;
    settings.onnx.intra_op_num_threads = 2;
    let mut fixture_media = settings.media;
    fixture_media.http_timeout_ms =
        std::num::NonZeroU64::new(std::env::var("CLUAIZ_FIXTURE_HTTP_TIMEOUT_MS")?.parse()?)
            .context("Fixture HTTP timeout")?;
    let resolver = cluaiz_media::source::SourceResolver::from_environment(fixture_media)?;
    let source = resolver
        .resolve(&std::env::var("CLUAIZ_MEDIA_AUDIO_URL")?, |_| Ok(()))
        .await?;
    let audio_data = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD
            .encode(source.read_bytes(fixture_media.limits).await?)
    );
    cluaiz_core::Settings::update(settings)?;
    let workspace = tempfile::tempdir()?;
    let permission = cluaiz_security::PermissionSchema {
        agent_security_mode: "strict".into(),
        api_auth: cluaiz_security::ApiAuth {
            required: true,
            ..Default::default()
        },
        ..Default::default()
    };
    permission.save()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    *state.permission_schema.write().await = permission;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let text = std::env::var("CLUAIZ_ONNX_TTS_TEXT")?;
    if text.trim().is_empty() {
        bail!("Non-empty TTS test text required");
    }
    let tts_request = json!({"model":ids[0],"task":"text_to_speech",
        "input_source":{"type":"text","data":text},"stream":false,"keep_alive":-1});
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/execute")
                .header("content-type", "application/json")
                .body(Body::from(tts_request.to_string()))?,
        )
        .await?;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    let (status, body) = post(&router, "/v1/audio/execute", tts_request.clone(), "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let response: Value = serde_json::from_str(&body)?;
    assert_eq!(response["model"], ids[0]);
    let wav = response["output"]["audio_data"]
        .as_str()
        .context("Real WAV result")?
        .to_owned();
    let decoded = cluaiz_media::audio::decode_audio(
        base64::engine::general_purpose::STANDARD
            .decode(wav.split_once(',').context("WAV URI")?.1)?,
        Default::default(),
    )?;
    assert!(decoded.samples().len() > decoded.sample_rate() as usize / 4);
    assert!(decoded.samples().iter().any(|v| v.abs() > 0.01));
    assert_eq!(
        state.host.onnx.resident_model().await?.as_deref(),
        Some(ids[0].as_str())
    );
    assert!(state.host.model_description().await.is_none());
    eprintln!(
        "REAL ONNX API TTS model={}, samples={}, rate={}",
        ids[0],
        decoded.samples().len(),
        decoded.sample_rate()
    );
    let (_, plan) = post(&router, "/v1/system/plan", json!({"model":ids[0]}), "user").await?;
    let plan: Value = serde_json::from_str(&plan)?;
    assert_eq!(plan["model_info"]["available_inputs"], json!(["text"]));
    assert_eq!(plan["model_info"]["outputs"], json!(["audio"]));
    let (status, body) = post(
        &router,
        "/v1/audio/speech",
        json!({"model":ids[0],"input":text}),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        serde_json::from_str::<Value>(&body)?["output"]["audio_data"]
            .as_str()
            .is_some_and(|s| s.starts_with("data:audio/wav;base64,"))
    );
    let (status, _) = post(
        &router,
        "/v1/audio/speech",
        json!({"model":ids[0],"input":text,"speed":"invalid"}),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut missing = tts_request.clone();
    missing["model"] = json!("missing-explicit-model");
    let (status, _) = post(&router, "/v1/audio/execute", missing, "user").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let mut unsupported = tts_request.clone();
    unsupported["parameters"] = json!({"voice_id":"unsupported"});
    let (status, _) = post(&router, "/v1/audio/execute", unsupported, "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut wrong = tts_request.clone();
    wrong["model"] = json!(ids[1]);
    let (status, _) = post(&router, "/v1/audio/execute", wrong, "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let stt_request = json!({"model":ids[1],"task":"speech_to_text",
        "input_source":{"type":"base64","data":audio_data},
        "parameters":{"language":"en"},"stream":true,"keep_alive":-1});
    let (status, body) = post(&router, "/v1/audio/execute", stt_request, "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("\"status\":\"error\""), "{body}");
    let terminal = body
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|v| v["status"] == "done")
        .context("STT terminal result missing")?;
    let transcript = terminal["result"]["output"]["text"]
        .as_str()
        .context("STT text")?;
    let expected = std::env::var("CLUAIZ_MEDIA_EXPECTED")?;
    if expected.trim().is_empty() {
        bail!("Ground truth required");
    }
    assert!(
        transcript
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase()),
        "{transcript}"
    );
    eprintln!("REAL ONNX API SSE STT: {transcript}");
    let (_, plan) = post(&router, "/v1/system/plan", json!({"model":ids[1]}), "user").await?;
    let plan: Value = serde_json::from_str(&plan)?;
    assert_eq!(plan["model_info"]["inputs"], json!(["audio"]));
    assert_eq!(plan["model_info"]["available_inputs"], json!(["audio"]));
    assert_eq!(plan["model_info"]["outputs"], json!(["text"]));
    eprintln!("REAL ONNX STT PLANNER: audio input / text output validated");
    let (status, body) = post(
        &router,
        "/v1/audio/transcriptions",
        json!({"model":ids[1],"input_source":{
        "type":"base64","data":audio_data}}),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let transcript = serde_json::from_str::<Value>(&body)?["output"]["text"]
        .as_str()
        .context("JSON STT text")?
        .to_owned();
    assert!(
        transcript
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase()),
        "{transcript}"
    );
    eprintln!("REAL ONNX API JSON STT: {transcript}");
    let entry = registry
        .installed_models
        .get(&ids[1])
        .context("STT entry")?;
    let processor: Value = serde_json::from_slice(&std::fs::read(
        PathBuf::from(&entry.local_dir).join("preprocessor_config.json"),
    )?)?;
    let rate = u32::try_from(
        processor["sampling_rate"]
            .as_u64()
            .context("Processor rate")?,
    )?;
    let count = usize::try_from(
        processor["n_samples"]
            .as_u64()
            .context("Processor sample limit")?,
    )? + 1;
    // Longer than the model's window: split into windows, never refused for its length (silence
    // has no transcript, so it ends with that error instead)
    let too_long = cluaiz_core::media::DecodedAudio::new(rate, 1, Arc::from(vec![0.0f32; count]))?;
    let mut events = state.host.onnx.dispatch(
        OnnxModel::from_registry(entry, AudioTask::SpeechToText)?,
        AudioTask::SpeechToText,
        AudioInput::Pcm(too_long),
        Default::default(),
        cluaiz_core::Settings::current(),
        Some(0),
        None,
    );
    match events.recv().await {
        Some(Err(message)) => assert!(!message.contains("supported window"), "{message}"),
        Some(Ok(_)) => {}
        None => anyhow::bail!("STT produced no event"),
    }
    assert!(state.host.onnx.resident_model().await?.is_none());
    let bad_source = json!({"model":ids[1],"input_source":{"type":"audio","data":"data:audio/wav;base64,AAAA"},"stream":false});
    let (status, _) = post(&router, "/v1/audio/execute", bad_source, "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let outside = tempfile::NamedTempFile::new()?;
    let local = json!({"model":ids[1],"input_source":{"type":"file","data":outside.path().to_str()},"stream":false});
    let (status, _) = post(&router, "/v1/audio/execute", local, "agent").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST); // owner path is accepted; empty file is not audio
    let mut unload = tts_request;
    unload["keep_alive"] = json!(0);
    let (status, body) = post(&router, "/v1/audio/execute", unload, "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(state.host.onnx.resident_model().await?.is_none());
    eprintln!("REAL ONNX API auth/model/task/options/decode/permission/keep_alive checks passed");
    Ok(())
}
