//! Opt-in actual HTTP/ONNX embedding acceptance with externally supplied packages.
#[path = "../../engine/media/tests/support/mod.rs"]
mod document_fixtures;
mod support;
use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use cluaiz_api::AppState;
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistry};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use support::post;
use tower::ServiceExt;

fn vector(body: &str, index: usize) -> Result<Vec<f32>> {
    let response: Value = serde_json::from_str(body)?;
    let values: Vec<f32> = serde_json::from_value(response["data"][index]["embedding"].clone())?;
    assert!(!values.is_empty() && values.iter().all(|v| v.is_finite()));
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4);
    Ok(values)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires isolated home, real ONNX image/text packages and test text"]
async fn real_router_selected_onnx_image_and_text_embeddings() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    if home.as_path() != std::path::Path::new(&std::env::var("CLUAIZ_MEDIA_TEST_HOME")?)
        || home.join("engine/config/model_registry.json").exists()
    {
        bail!("Empty isolated fixture home required");
    }
    let mut registry = ModelRegistry::default();
    let mut ids = Vec::new();
    for variable in ["CLUAIZ_ONNX_TEXT_MODEL", "CLUAIZ_ONNX_IMAGE_MODEL"] {
        let model = PathBuf::from(std::env::var(variable)?);
        let dir = model.parent().context("Model parent")?;
        if dir.starts_with(&home) {
            bail!("Model weights must remain outside the home");
        }
        // Actual graph evidence adds the image task while preserving ingest category.
        let entry = InstalledStateRegistry::entry_for_folder(
            if variable == "CLUAIZ_ONNX_IMAGE_MODEL" {
                "ingest"
            } else {
                "embedding"
            },
            dir,
            model.file_name().and_then(|v| v.to_str()),
            "",
        )
        .context("Registry entry")?;
        ids.push(entry.id.clone());
        registry.installed_models.insert(entry.id.clone(), entry);
    }
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.onnx.n_gpu_layers = 0;
    settings.onnx.intra_op_num_threads = 2;
    cluaiz_core::Settings::update(settings.clone())?;
    let workspace = tempfile::tempdir()?;
    let policy = cluaiz_security::PermissionSchema {
        agent_security_mode: "strict".into(),
        api_auth: cluaiz_security::ApiAuth {
            required: true,
            ..Default::default()
        },
        ..Default::default()
    };
    policy.save()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    *state.permission_schema.write().await = policy;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let (status, body) = post(&router, "/v1/system/plan", json!({"model":ids[0]}), "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plan: Value = serde_json::from_str(&body)?;
    assert_eq!(plan["status"], "success");
    assert_eq!(plan["model_info"]["inputs"], json!(["text"]));
    assert_eq!(plan["model_info"]["available_inputs"], json!([]));
    assert!(plan["model_info"]["max_input_context"].is_null());
    assert!(state.host.onnx.resident_model().await?.is_none());
    let text = std::env::var("CLUAIZ_EMBEDDING_TEST_TEXT")?;
    let other = std::env::var("CLUAIZ_EMBEDDING_TEST_OTHER")?;
    let request = json!({"model":ids[0],"input":[text,text,other],"keep_alive":-1});
    let unauth = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))?,
        )
        .await?;
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let (status, body) = post(&router, "/v1/embeddings", request, "user").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let first = vector(&body, 0)?;
    assert_eq!(first, vector(&body, 1)?);
    assert_ne!(first, vector(&body, 2)?);
    let response: Value = serde_json::from_str(&body)?;
    assert_eq!(response["model"], ids[0]);
    assert!(
        response["usage"]["prompt_tokens"]
            .as_u64()
            .context("Actual token count")?
            > 0
    );
    println!("REAL ONNX TEXT: {} finite normalized dimensions; repeated input agrees, changed input differs",first.len());
    // The same native extraction service feeds actual ONNX vectors through its
    // public API contract; a file is never represented by its filename.
    let mut documents = vec![
        (
            "application/pdf",
            document_fixtures::pdf(&[Some("The attached document records order ALPHA731")])?,
        ),
        (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            document_fixtures::docx(document_fixtures::DOCX_XML)?,
        ),
    ];
    if std::env::var_os("CLUAIZ_NATIVE_MEDIA_TEST_PACKAGE").is_some() {
        let assets = cluaiz_media::native_assets::NativeAssets::from_environment()?;
        let pdf = document_fixtures::pdf(&[Some("Scanned receipt code 741923")])?;
        let image =
            cluaiz_media::documents::pdf_render::with_document(&pdf, &assets, |document| {
                document.render_page(1, cluaiz_core::media_limits::MediaLimits::default())
            })?;
        documents.push((
            "application/pdf",
            document_fixtures::raster::pdf(&image, None)?,
        ));
    }
    for (mime, bytes) in documents {
        let source = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let (status, body) = post(&router, "/v1/ingest", json!({"source":source}), "user").await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        let document: Value = serde_json::from_str(&body)?;
        assert_eq!(document["status"], "success");
        let chunks = document["processed_sources"][0]["chunks"].clone();
        let scanned = document["processed_sources"][0]["parts"][0]["method"] == "pdf_ocr";
        if scanned {
            assert!(chunks.to_string().contains("741923"));
        }
        assert!(!chunks
            .as_array()
            .context("Real document chunks")?
            .is_empty());
        let (status, body) = post(
            &router,
            "/v1/embeddings",
            json!({"model":ids[0],"input":chunks,"keep_alive":-1}),
            "user",
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(vector(&body, 0)?.len(), first.len());
        println!("REAL DOCUMENT ONNX: {mime} -> native extracted chunks -> {} finite normalized embedding dimensions", first.len());
        if scanned {
            println!("REAL NATIVE OCR ONNX: raster-only PDF -> ingest -> selected 384-dimension text graph");
        }
    }
    assert_eq!(
        state.host.onnx.resident_model().await?,
        Some(ids[0].clone())
    );
    let (_, body) = post(&router, "/v1/system/plan", json!({"model":ids[0]}), "user").await?;
    let plan: Value = serde_json::from_str(&body)?;
    assert_eq!(plan["model_info"]["available_inputs"], json!(["text"]));
    assert_eq!(plan["model_info"]["outputs"], json!(["embedding"]));
    let mut images = Vec::new();
    for color in [image::Rgb([255u8, 0, 0]), image::Rgb([0u8, 0, 255])] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 16, color))
            .write_to(&mut bytes, image::ImageOutputFormat::Png)?;
        images.push(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
        ));
    }
    let mut vectors = Vec::new();
    for source in &images {
        let (status, body) = post(
            &router,
            "/v1/embeddings",
            json!({"model":ids[1],
            "input":[{"type":"image","source":source}],"keep_alive":-1}),
            "user",
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        vectors.push(vector(&body, 0)?);
    }
    assert_ne!(vectors[0], vectors[1]);
    let (_, body) = post(&router, "/v1/system/plan", json!({"model":ids[1]}), "user").await?;
    let plan: Value = serde_json::from_str(&body)?;
    assert_eq!(plan["model_info"]["inputs"], json!(["text", "image"]));
    assert_eq!(plan["model_info"]["available_inputs"], json!(["image"]));
    assert_eq!(plan["model_info"]["outputs"], json!(["embedding"]));
    assert!(plan["model_info"]["supported_attachments"]
        .as_array()
        .context("Image formats")?
        .contains(&json!("png")));
    assert!(plan["model_info"]["missing_assets"]
        .as_array()
        .context("Helpers")?
        .is_empty());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/models")
                .header("authorization", "Bearer test_session_token")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
    let models: Value = serde_json::from_slice(&body)?;
    let listed = models["data"]
        .as_array()
        .context("Listed models")?
        .iter()
        .find(|m| m["id"] == ids[1])
        .context("Image listed")?;
    assert_eq!(listed["task_support"]["available_inputs"], json!(["image"]));
    let mut changed = settings.clone();
    changed.onnx.intra_op_num_threads += 1;
    cluaiz_core::Settings::update(changed)?;
    let (_, body) = post(&router, "/v1/system/plan", json!({"model":ids[1]}), "user").await?;
    assert_eq!(
        serde_json::from_str::<Value>(&body)?["model_info"]["available_inputs"],
        json!([])
    );
    cluaiz_core::Settings::update(settings)?;
    let incomplete = tempfile::tempdir()?;
    std::fs::hard_link(
        std::env::var("CLUAIZ_ONNX_IMAGE_MODEL")?,
        incomplete.path().join("renamed.onnx"),
    )?;
    let entry = InstalledStateRegistry::entry_for_folder(
        "ingest",
        incomplete.path(),
        Some("renamed.onnx"),
        "",
    )
    .context("Incomplete package")?;
    let incomplete_id = entry.id.clone();
    InstalledStateRegistry::register_model(entry).map_err(anyhow::Error::msg)?;
    let (_, body) = post(
        &router,
        "/v1/system/plan",
        json!({"model":incomplete_id}),
        "user",
    )
    .await?;
    let plan: Value = serde_json::from_str(&body)?;
    assert_eq!(plan["model_info"]["available_inputs"], json!([]));
    let missing = plan["model_info"]["missing_assets"]
        .as_array()
        .context("Missing helpers")?;
    assert!(
        missing.contains(&json!("tokenizer.json"))
            && missing.contains(&json!("preprocessor_config.json"))
    );
    let resident = state.host.onnx.resident_model().await?;
    let (status, body) = post(
        &router,
        "/v1/embeddings",
        json!({"model":incomplete_id,
        "input":[{"type":"image","source":"https://invalid.example/not-fetched.png"}]}),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let error: Value = serde_json::from_str(&body)?;
    assert_eq!(error["error"]["type"], "invalid_model");
    assert!(error["error"]["message"]
        .as_str()
        .context("Missing helper error")?
        .contains("preprocessor_config.json"));
    assert_eq!(state.host.onnx.resident_model().await?, resident);
    println!("REAL ONNX PREFLIGHT: missing helpers -> HTTP400 before source decoding/native load; existing resident unchanged");
    println!("REAL ONNX PLANNER: unloaded/validated/settings-change/missing-helper states and ingest-category image task passed");
    println!(
        "REAL ONNX IMAGE: {} finite normalized dimensions; red and blue differ",
        vectors[0].len()
    );
    for (request, expected) in [
        (
            json!({"model":"missing-package","input":text}),
            StatusCode::NOT_FOUND,
        ),
        (json!({"model":ids[0],"input":[]}), StatusCode::BAD_REQUEST),
        (
            json!({"model":ids[0],"input":text,"encoding_format":"base64"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"model":ids[1],"input":[{"type":"image","source":"data:image/png;base64,AAAA"}]}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"model":ids[1],"input":[{"type":"image","source":std::env::var("CLUAIZ_ONNX_IMAGE_MODEL")?}]}),
            StatusCode::FORBIDDEN,
        ),
        (
            json!({"model":ids[0],"input":[{"type":"image","source":images[0]}]}),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            json!({"model":ids[1],"input":text}),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        let (status, body) = post(&router, "/v1/embeddings", request, "agent").await?;
        assert_eq!(status, expected, "{body}");
    }
    let (status, body) = post(
        &router,
        "/v1/embeddings",
        json!({"model":ids[0],"input":text,"keep_alive":0}),
        "user",
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    vector(&body, 0)?;
    assert_eq!(state.host.onnx.resident_model().await?, None);
    let (_, body) = post(&router, "/v1/system/plan", json!({"model":ids[0]}), "user").await?;
    assert_eq!(
        serde_json::from_str::<Value>(&body)?["model_info"]["available_inputs"],
        json!([])
    );
    println!(
        "REAL ONNX API: auth/model/encoding/decode/permission/modality/retention checks passed"
    );
    cluaiz_media::documents::release_native_providers()?;
    Ok(())
}
