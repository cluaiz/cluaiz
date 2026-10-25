//! Actual HTTP document extraction and opt-in document-to-native-model acceptance.
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
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

fn inline(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[tokio::test]
async fn test_router_document_extraction_content_errors_auth_and_workspace() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    state.permission_schema.write().await.api_auth.required = true;
    state.permission_schema.write().await.agent_security_mode = "strict".into();
    let router = cluaiz_api::routes::build_api_router(state);
    let pdf =
        document_fixtures::pdf(&[Some("First order ALPHA731"), Some("Second order BETA942")])?;
    let source = inline("application/pdf", &pdf);
    let request = json!({"source":source,"chunking_strategy":{"max_chunk_size":32}});
    let denied = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cep")
                .header("content-type", "application/json")
                .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"ingest.run","params":request}).to_string()))?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let body = support::ingest(&router, request).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    let response: Value = serde_json::from_str(&body)?;
    assert_eq!(response["status"], "success");
    let item = &response["processed_sources"][0];
    assert_eq!(item["parts"][0]["page"], 1);
    assert_eq!(item["parts"][1]["page"], 2);
    assert!(item["parts"][0]["text"]
        .as_str()
        .context("PDF text")?
        .contains("ALPHA731"));
    assert!(item["chunks"]
        .as_array()
        .context("Chunks")?
        .iter()
        .all(|chunk| chunk.as_str().is_some_and(|text| text.len() <= 32)));
    let docx = document_fixtures::docx(document_fixtures::DOCX_XML)?;
    let body = support::ingest(&router, json!({"source":inline(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document", &docx)})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    assert!(
        serde_json::from_str::<Value>(&body)?["processed_sources"][0]["parts"][0]["text"]
            .as_str()
            .context("DOCX text")?
            .contains("Hello & नमस्ते")
    );
    for (mime, bytes, expected) in [
        (
            "application/msword",
            document_fixtures::doc("Actual legacy content नमस्ते 3187\r", false, true)?,
            "नमस्ते 3187",
        ),
        (
            "text/plain",
            b"\xef\xbb\xbfconst actual_value = 3187;".to_vec(),
            "actual_value = 3187",
        ),
        (
            "text/plain",
            [
                vec![0xff, 0xfe],
                "Unicode नमस्ते 3187"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            ]
            .concat(),
            "नमस्ते 3187",
        ),
    ] {
        let body = support::ingest(&router, json!({"source":inline(mime, &bytes)})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
        let response: Value = serde_json::from_str(&body)?;
        assert_eq!(response["status"], "success", "{body}");
        assert!(
            response["processed_sources"][0]["parts"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains(expected)),
            "{body}"
        );
    }
    let mixed = document_fixtures::pdf(&[Some("Known page content"), None])?;
    let body = support::ingest(&router, json!({"source":inline("application/pdf", &mixed)})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    let response: Value = serde_json::from_str(&body)?;
    assert_eq!(response["status"], "partial");
    assert!(!response["processed_sources"][0]["parts"][1]["omissions"]
        .as_array()
        .context("Omissions")?
        .is_empty());
    for request in [
        json!({"source":[]}),
        json!({"source":source,"chunking_strategy":{"max_chunk_size":0}}),
        json!({"source":source,"output_controls":{"return_embeddings":true}}),
        json!({"source":inline("application/pdf", b"%PDF-1.4 corrupt")}),
        json!({"source":inline("application/msword", b"not a compound Word file\0")}),
        json!({"source":inline("text/plain", b"\xff\xfe\x00\xd8")}),
        json!({"source":inline("text/plain", b"binary\0content")}),
        json!({"source":"missing.pdf"}),
    ] {
        let body = support::ingest(&router, request).await?.expect_err("ingest must fail").to_string();
        assert!(body.contains("\"code\":-32602"), "{body}");
    }
    let local = workspace.path().join("local.pdf");
    std::fs::write(&local, pdf)?;
    let body = support::ingest(&router, json!({"source":"local.pdf"})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    std::fs::write(
        workspace.path().join("source.ts"),
        "const actual_order = 23;",
    )?;
    let body = support::ingest(&router, json!({"source":"source.ts"})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    assert!(body.contains("actual_order = 23"), "{body}");
    let body = support::ingest(&router, json!({"source":"local.pdf"})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    let outside = tempfile::tempdir()?;
    let external = outside.path().join("owner.md");
    std::fs::write(&external, "external owner document")?;
    let body = support::ingest(&router, json!({"source":external})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    assert!(body.contains("external owner document"), "{body}");
    let body = support::ingest(&router, json!({"source":source,"output_controls":{"return_text":false}})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
    let response: Value = serde_json::from_str(&body)?;
    assert!(response["processed_sources"][0].get("chunks").is_none());
    assert!(response["processed_sources"][0]["parts"][0]
        .get("text")
        .is_none());
    println!("REAL DOCUMENT HTTP: PDF/DOCX content/order, partial pages, auth, local approval/denial, corrupt/unsupported options and metadata-only output passed");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires isolated CLUAIZ_HOME / CLUAIZ_MEDIA_TEST_HOME and a real text GGUF"]
async fn real_router_pdf_attachment_reaches_text_only_llama() -> Result<()> {
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    if home.as_path() != std::path::Path::new(&std::env::var("CLUAIZ_MEDIA_TEST_HOME")?)
        || home.join("engine/config/model_registry.json").exists()
    {
        bail!("Empty isolated fixture home required");
    }
    let weights = PathBuf::from(std::env::var("CLUAIZ_DOCUMENT_TEST_MODEL")?);
    if weights.starts_with(&home) {
        bail!("Read-only weights must be outside the fixture home");
    }
    let package = tempfile::tempdir()?;
    let primary = package
        .path()
        .join(weights.file_name().context("GGUF filename")?);
    std::fs::hard_link(weights, &primary)?;
    // This isolated package has exactly one text GGUF and no projector.
    let entry = cluaiz_models::registry::InstalledStateRegistry::entry_for_folder(
        "chat",
        package.path(),
        primary.file_name().and_then(|name| name.to_str()),
        "",
    )
    .context("Text package registration")?;
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
    let policy = cluaiz_security::PermissionSchema {
        agent_security_mode: "strict".into(),
        stream_telemetry: false,
        api_auth: cluaiz_security::ApiAuth {
            required: true,
            ..Default::default()
        },
        ..Default::default()
    };
    policy.save()?;
    let workspace = tempfile::tempdir()?;
    let state = Arc::new(AppState::new_test_state(workspace.path().to_path_buf()));
    *state.permission_schema.write().await = policy;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let native_assets = if std::env::var_os("CLUAIZ_NATIVE_MEDIA_TEST_PACKAGE").is_some() {
        Some(cluaiz_media::native_assets::NativeAssets::from_environment()?)
    } else {
        None
    };
    for (code, stream) in [("741923", false), ("826417", true)] {
        let pdf = document_fixtures::pdf(&[Some(&format!("The recorded order code is {code}."))])?;
        let pdf = if let Some(assets) = &native_assets {
            let image =
                cluaiz_media::documents::pdf_render::with_document(&pdf, assets, |document| {
                    document.render_page(1, cluaiz_core::media_limits::MediaLimits::default())
                })?;
            let scanned = document_fixtures::raster::pdf(&image, None)?;
            assert!(!scanned
                .windows(code.len())
                .any(|window| window == code.as_bytes()));
            let body = support::ingest(&router, json!({"source":inline("application/pdf", &scanned)})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
            let extracted: Value = serde_json::from_str(&body)?;
            assert_eq!(extracted["status"], "success");
            assert_eq!(
                extracted["processed_sources"][0]["parts"][0]["method"],
                "pdf_ocr"
            );
            assert!(extracted["processed_sources"][0]["parts"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains(code)));
            let png = document_fixtures::raster::encoded(&image, image::ImageOutputFormat::Png)?;
            let body = support::ingest(&router, json!({"source":inline("image/png", &png)})).await?.map_err(|e| anyhow::anyhow!("{e}"))?.to_string();
            let extracted: Value = serde_json::from_str(&body)?;
            assert_eq!(
                extracted["processed_sources"][0]["parts"][0]["method"],
                "native_ocr"
            );
            assert!(extracted["processed_sources"][0]["parts"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains(code)));
            println!(
                "REAL NATIVE OCR HTTP: scanned PDF and PNG -> ingest API -> actual code {code}"
            );
            scanned
        } else {
            pdf
        };
        let source = if stream {
            inline("application/pdf", &pdf)
        } else {
            std::fs::write(workspace.path().join("order.pdf"), &pdf)?;
            "order.pdf".to_owned()
        };
        let request = json!({"model":id,"messages":[{"role":"user","content":[
            {"type":"file_url","file_url":{"url":source}},
            {"type":"text","text":"Read the attached document. What is the recorded order code? Reply with the code only."}]}],
            "stream":stream,"temperature":0,"max_tokens":48,"think_mode":"off"});
        let (status, body) =
            support::post(&router, "/v1/chat/completions", request, "user").await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body.contains("\"error\""), "{body}");
        let mut answer = String::new();
        if stream {
            for line in body.lines().filter_map(|line| line.strip_prefix("data: ")) {
                if line == "[DONE]" {
                    continue;
                }
                let chunk: Value = serde_json::from_str(line)?;
                if let Some(text) = chunk["choices"][0]["delta"]["content"].as_str() {
                    answer.push_str(text);
                }
            }
        } else {
            let response: Value = serde_json::from_str(&body)?;
            answer = response["choices"][0]["message"]["content"]
                .as_str()
                .context("Actual assistant text")?
                .to_owned();
        }
        assert!(
            answer.contains(code),
            "Actual model answer must depend on document content: {answer}"
        );
        if stream {
            assert!(body.contains("[DONE]"), "{body}");
        }
        println!("REAL DOCUMENT CHAT: selected text-only GGUF answered code {code}; stream={stream}; scanned={}; no projector in package", native_assets.is_some());
    }
    for (mime, bytes, code) in [
        (
            "application/msword",
            document_fixtures::doc("The recorded order code is 593187.\r", false, true)?,
            "593187",
        ),
        (
            "text/plain",
            b"// Recorded order code\nconst order_code = 682954;".to_vec(),
            "682954",
        ),
    ] {
        let (status, body) = support::post(&router, "/v1/chat/completions", json!({
            "model":id, "messages":[{"role":"user", "content":[
                {"type":"file_url", "file_url":{"url":inline(mime, &bytes)}},
                {"type":"text", "text":"Read the attached document. What is the recorded order code? Reply with the code only."}
            ]}], "temperature":0, "max_tokens":48, "think_mode":"off"
        }), "user").await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        let response: Value = serde_json::from_str(&body)?;
        let answer = response["choices"][0]["message"]["content"]
            .as_str()
            .context("Actual attached-document answer")?;
        assert!(
            answer.contains(code),
            "Actual model answer must depend on {mime} content: {answer}"
        );
        println!("REAL DOCUMENT CHAT: {mime} -> selected text-only GGUF -> actual answer {answer}; no projector");
    }
    let mixed = document_fixtures::pdf(&[Some("Known text"), None])?;
    let (status, body) = support::post(&router, "/v1/chat/completions", json!({"model":id,"messages":[{"role":"user","content":[
        {"type":"file_url","file_url":{"url":inline("application/pdf", &mixed)}}, {"type":"text","text":"Summarize the full document"}]}]}), "user").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("partial"), "{body}");
    let plan = support::cep(&router, "system.plan", json!({"model":id}))
        .await?
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let text_input = plan["model_info"]["inputs"]
        .as_array()
        .context("Inputs")?
        .iter()
        .find(|input| input["type"] == "text")
        .context("Text input")?;
    assert_eq!(text_input, &json!({"type":"text","available":true}));
    assert!(text_input.get("formats").is_none());
    assert!(plan["model_info"].get("supported_attachments").is_none());
    state.host.unload_model().await;
    cluaiz_media::documents::release_native_providers()?;
    println!("REAL DOCUMENT CHAT: partial extraction rejected; planner reports text input without document extension aliases");
    Ok(())
}
