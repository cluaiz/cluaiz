//! Actual installed-package acceptance for optional ingest processing; weights stay external.
#[path = "../../engine/media/tests/support/mod.rs"]
mod documents;
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
use std::{io::Cursor, path::PathBuf, sync::Arc};
use tower::ServiceExt;

fn source(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
fn png(color: [u8; 3]) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb(color)))
        .write_to(&mut bytes, image::ImageOutputFormat::Png)?;
    Ok(bytes.into_inner())
}
fn vectors(item: &Value) -> Result<Vec<Vec<f32>>> {
    let mut result = Vec::new();
    for batch in item["embeddings"].as_array().context("Embedding batches")? {
        for value in batch["data"].as_array().context("Embedding data")? {
            let vector: Vec<f32> = serde_json::from_value(value["embedding"].clone())?;
            assert!(!vector.is_empty() && vector.iter().all(|x| x.is_finite()));
            let norm = vector
                .iter()
                .map(|x| f64::from(*x).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!((norm - 1.0).abs() < 1e-4);
            result.push(vector);
        }
    }
    Ok(result)
}
async fn ingest(router: &axum::Router, payload: Value) -> Result<Value> {
    let body = support::ingest(&router, payload).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    let value: Value = serde_json::from_str(&body)?;
    assert_eq!(value["status"], "success", "{value}");
    Ok(value["processed_sources"][0].clone())
}

async fn chat_pdf(
    router: &axum::Router,
    model: &str,
    pdf: &str,
    instruction: &str,
    expected: &[&str],
) -> Result<()> {
    let (status, body) = support::post(
        router,
        "/v1/chat/completions",
        json!({
            "model":model,"stream":false,"temporary_chat":"strict","max_tokens":96,
            "temperature":0,"think_mode":"off","messages":[{"role":"user","content":[
                {"type":"file_url","file_url":{"url":pdf}},
                {"type":"text","text":instruction}
            ]}]
        }),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let response: Value = serde_json::from_str(&body)?;
    let answer = response["choices"][0]["message"]["content"]
        .as_str()
        .context("Chat PDF answer")?;
    assert!(
        expected
            .iter()
            .all(|value| answer.to_lowercase().contains(&value.to_lowercase())),
        "{body}"
    );
    Ok(())
}

async fn pdf_preparation(pdf: &str, mut settings: cluaiz_core::ActiveUserSettings) -> Result<()> {
    use cluaiz_core::{
        media::{InputPart, PreparedPart},
        ChatEnvelope, ChatMessage,
    };
    let request = || ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: vec![InputPart::Document { source: pdf.into() }],
        }],
        samplers: Default::default(),
        think_mode: "off".into(),
        reasoning_effort: cluaiz_core::ThinkingBudget::Tokens(0),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
        tools: None,
    };
    let resolver = cluaiz_media::source::SourceResolver::from_environment(settings.media)?
        .with_document_images(true);
    let prepared = cluaiz_media::request::prepare_chat(request(), &resolver, |_| Ok(())).await?;
    let parts = &prepared.messages[0].content;
    assert_eq!(
        parts
            .iter()
            .filter(|p| matches!(p, PreparedPart::Image { .. }))
            .count(),
        2
    );
    let labels = parts
        .iter()
        .filter_map(|p| {
            if let PreparedPart::Text { text } = p {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["Document page 1:\n", "Document page 2:\n"]);
    assert!(
        !labels.join("").contains("ALPHA731"),
        "PDF pixels must not be replaced with native/OCR text"
    );
    settings.media.limits =
        cluaiz_core::media_limits::MediaLimits::new(settings.media.limits.max_input_bytes(), 1024)?;
    let limited = cluaiz_media::source::SourceResolver::from_environment(settings.media)?
        .with_document_images(true);
    assert!(
        cluaiz_media::request::prepare_chat(request(), &limited, |_| Ok(()))
            .await
            .is_err(),
        "Page workspace must respect the decoded budget"
    );
    let signal = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancelled = resolver.with_cancellation(signal);
    assert!(
        cluaiz_media::request::prepare_chat(request(), &cancelled, |_| Ok(()))
            .await
            .is_err()
    );
    println!("REAL CHAT PDF PREPARATION: two pixel pages in order; byte-limit and cancellation errors retained");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an isolated home, source registry and actual ONNX/GGUF packages"]
async fn real_ingest_selected_models_and_native_fallback() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    if home != PathBuf::from(std::env::var("CLUAIZ_MEDIA_TEST_HOME")?)
        || home.join("config/model_registry.json").exists()
    {
        bail!("Empty isolated fixture home required");
    }
    let original: ModelRegistry =
        serde_json::from_slice(&std::fs::read(std::env::var("CLUAIZ_INGEST_REGISTRY")?)?)?;
    let ids = [
        "CLUAIZ_INGEST_TEXT_MODEL",
        "CLUAIZ_INGEST_IMAGE_MODEL",
        "CLUAIZ_INGEST_GGUF_MODEL",
        "CLUAIZ_INGEST_VISION_MODEL",
        "CLUAIZ_INGEST_INCOMPLETE_VISION_MODEL",
    ]
    .into_iter()
    .map(std::env::var)
    .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut registry = ModelRegistry::default();
    for id in &ids {
        let entry = original
            .installed_models
            .get(id)
            .context("Selected installed package")?
            .clone();
        if PathBuf::from(&entry.local_dir).starts_with(&home) {
            bail!("Weights must remain external");
        }
        registry.installed_models.insert(id.clone(), entry);
    }
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = None;
    settings.optimization.custom_context_window = Some(4096);
    settings.optimization.n_threads = Some(8);
    settings.optimization.n_threads_batch = Some(8);
    settings.onnx.n_gpu_layers = 0;
    settings.onnx.intra_op_num_threads = 4;
    cluaiz_core::Settings::update(settings.clone())?;
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    state.permission_schema.write().await.api_auth.required = true;
    state.permission_schema.write().await.agent_security_mode = "strict".into();
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let inline = source(
        "text/plain",
        b"Catalog order ALPHA731 describes a blue jacket.",
    );
    let denied = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingest")
                .header("content-type", "application/json")
                .body(Body::from(json!({"source":inline}).to_string()))?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let missing = support::ingest(&router, json!({"source":inline,"model":"uninstalled-ingest-model"})).await?.expect_err("ingest must fail").to_string();
    assert!(missing.contains("model_not_found"), "{missing}");
    let native = ingest(&router, json!({"source":inline})).await?;
    assert!(native["chunks"][0]
        .as_str()
        .context("Native text")?
        .contains("ALPHA731"));
    assert!(native.get("model").is_none());

    let pdf = source(
        "application/pdf",
        &documents::pdf(&[Some("Invoice ALPHA731"), Some("Order BETA942")])?,
    );
    let docx = source(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        &documents::docx(documents::DOCX_XML)?,
    );
    pdf_preparation(&pdf, settings.clone()).await?;
    for input in [&inline, &pdf, &docx] {
        let item = ingest(
            &router,
            json!({"source":input,"model":ids[0],"output_controls":{"return_embeddings":true}}),
        )
        .await?;
        assert_eq!(item["model"], ids[0]);
        assert_eq!(item["processing"], "text_embeddings");
        let actual = vectors(&item)?;
        assert_eq!(
            actual.len(),
            item["chunks"].as_array().context("Native chunks")?.len()
        );
        let (status, body) = support::post(
            &router,
            "/v1/embeddings",
            json!({"model":ids[0],"input":item["chunks"],"keep_alive":0}),
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        let direct: Value = serde_json::from_str(&body)?;
        let first: Vec<f32> = serde_json::from_value(direct["data"][0]["embedding"].clone())?;
        assert_eq!(actual[0], first);
        println!("REAL INGEST ONNX TEXT: {} -> {} vectors, {} dimensions; same actual chunks match /embeddings",item["mime_type"],actual.len(),first.len());
    }
    let item = ingest(&router, json!({"source":inline,"model":ids[2]})).await?;
    println!(
        "REAL INGEST GGUF TEXT: {} dimensions",
        vectors(&item)?[0].len()
    );
    let (status, body) = support::post(
        &router,
        "/v1/embeddings",
        json!({"model":ids[2],"input":"release resident","keep_alive":0}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");

    let red = source("image/png", &png([255, 0, 0])?);
    let blue = source("image/png", &png([0, 0, 255])?);
    let request = |input: &str| json!({"source":input,"model":ids[1],"output_controls":{"return_text":false,"return_embeddings":true}});
    let red_item = ingest(&router, request(&red)).await?;
    let blue_item = ingest(&router, request(&blue)).await?;
    let repeat = ingest(&router, request(&blue)).await?;
    assert_ne!(vectors(&red_item)?, vectors(&blue_item)?);
    assert_eq!(vectors(&blue_item)?, vectors(&repeat)?);
    assert!(red_item.get("chunks").is_none());
    println!("REAL INGEST ONNX IMAGE: {} dimensions; red/blue differ, repeated blue agrees; no invented OCR text",vectors(&red_item)?[0].len());
    let page_vectors = ingest(&router, request(&pdf)).await?;
    assert_eq!(vectors(&page_vectors)?.len(), 2);
    assert_eq!(page_vectors["embeddings"][0]["page"], 1);
    assert_eq!(page_vectors["embeddings"][1]["page"], 2);
    println!("REAL INGEST ONNX PDF: 2 rendered pages -> actual page vectors with page provenance");
    let (status, body) = support::post(
        &router,
        "/v1/embeddings",
        json!({"model":ids[1],"input":[{"type":"image","source":blue}],"keep_alive":0}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert!(state.host.model_description().await.is_none());
    let body = support::ingest(&router, json!({"model":ids[4],"source":red})).await?.expect_err("ingest must fail").to_string();
    assert!(body.contains("\"error\""), "{body}");
    assert!(body.contains("projector"), "{body}");
    assert!(
        state.host.model_description().await.is_none(),
        "Missing projector must fail before backbone load"
    );
    println!("REAL INGEST NEGATIVE: incomplete installed vision package rejected with projector error before loading weights");

    let native_model = ingest(&router,json!({"source":pdf,"model":ids[4],"vision_settings":{
        "use_vision":false,"system_instruction":"Return both order codes from the document. Copy the codes exactly.","max_tokens":64}})).await?;
    let native_answer = native_model["chunks"].to_string();
    assert!(
        native_answer.contains("ALPHA731") && native_answer.contains("BETA942"),
        "{native_model}"
    );
    println!("REAL INGEST NATIVE+MODEL: backbone-only package reads native-extracted PDF text without pretending to see pixels");
    chat_pdf(
        &router,
        &ids[4],
        &pdf,
        "Return both order codes from the document. Copy the codes exactly.",
        &["ALPHA731", "BETA942"],
    )
    .await?;
    println!("REAL CHAT PDF NATIVE: text-only backbone reads native-extracted codes");

    for (input, expected) in [(&red, "red"), (&blue, "blue")] {
        let item = ingest(&router,json!({"source":input,"model":ids[3],"vision_settings":{
            "system_instruction":"Name the single solid color in this image. Reply with one color word.","max_tokens":32}})).await?;
        assert_eq!(item["model"], ids[3]);
        let answer = item["chunks"]
            .as_array()
            .context("Vision chunks")?
            .iter()
            .filter_map(Value::as_str)
            .collect::<String>();
        assert!(answer.to_lowercase().contains(expected), "{item}");
        println!("REAL INGEST VISION: {expected} -> {answer}");
    }
    let item = ingest(&router,json!({"source":pdf,"model":ids[3],"vision_settings":{
        "system_instruction":"Read the order code from this page. Return only the code.","max_tokens":64}})).await?;
    assert_eq!(
        item["parts"].as_array().context("Vision PDF parts")?.len(),
        2
    );
    let combined = item["chunks"].to_string();
    assert!(
        combined.contains("ALPHA731") && combined.contains("BETA942"),
        "{item}"
    );
    println!("REAL INGEST VISION PDF: both page codes returned by the selected model");
    chat_pdf(
        &router,
        &ids[3],
        &pdf,
        "Return both order codes from the document. Copy the codes exactly.",
        &["ALPHA731", "BETA942"],
    )
    .await?;
    println!("REAL CHAT PDF VISION: selected image-capable model reads both rendered page codes through file_url");
    for (color, expected) in [([255, 0, 0], "red"), ([0, 0, 255], "blue")] {
        let pixels = cluaiz_media::image::decode_image(&png(color)?, settings.media.limits)?;
        let graphic_pdf = source("application/pdf", &documents::raster::pdf(&pixels, None)?);
        chat_pdf(
            &router,
            &ids[3],
            &graphic_pdf,
            "Name the single solid color shown on the PDF page. Reply with one color word.",
            &[expected],
        )
        .await?;
        println!("REAL CHAT PDF GRAPHIC: raster-only {expected} page recognized without an embedded text layer");
    }
    state.host.unload_model().await;
    cluaiz_media::documents::release_native_providers()?;
    println!("REAL INGEST: native fallback, exact selected ONNX/GGUF executors, PDF/image/Word/text and authentication passed");
    Ok(())
}
