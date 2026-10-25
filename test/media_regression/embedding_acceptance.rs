//! Current executable acceptance; reuse the existing isolated daemon/client ownership.
use super::*;

#[tokio::test]
#[ignore = "requires isolated home, final executable and explicit GGUF/ONNX/image/chat model paths"]
async fn real_final_executable_embeddings_and_vision() -> Result<()> {
    let home = env_path("CLUAIZ_HOME")?;
    assert_eq!(home, env_path("CLUAIZ_MEDIA_TEST_HOME")?);
    if home.join("engine/config/model_registry.json").exists() {
        bail!("Empty isolated home required");
    }
    let mut registry = ModelRegistry::default();
    let mut ids = Vec::new();
    for (category, variable) in [
        ("embedding", "CLUAIZ_GGUF_EMBEDDING_MODEL"),
        ("embedding", "CLUAIZ_ONNX_TEXT_MODEL"),
        ("ingest", "CLUAIZ_ONNX_IMAGE_MODEL"),
        ("chat", "CLUAIZ_MEDIA_MODEL"),
    ] {
        let path = env_path(variable)?;
        let directory = path.parent().context("Model parent")?;
        if directory.starts_with(&home) {
            bail!("Weights must remain outside mutable test home");
        }
        let entry = InstalledStateRegistry::entry_for_folder(
            category,
            directory,
            path.file_name().and_then(|name| name.to_str()),
            "",
        )
        .context("Installed model entry")?;
        ids.push(entry.id.clone());
        registry.installed_models.insert(entry.id.clone(), entry);
    }
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.custom_context_window = Some(512);
    settings.optimization.n_threads = Some(2);
    settings.optimization.n_threads_batch = Some(2);
    settings.onnx.n_gpu_layers = 0;
    settings.onnx.intra_op_num_threads = 2;
    cluaiz_core::Settings::update(settings)?;
    configure_test_permissions()?;
    let (_daemon, client) = start_daemon(&home).await?;
    let unauthorized = client
        .http
        .post(format!("{}/v1/embeddings", client.base))
        .json(&json!({"model":ids[0],"input":"ignored"}))
        .send()
        .await?;
    assert_eq!(unauthorized.status(), 401);
    let text = std::env::var("CLUAIZ_EMBEDDING_TEST_TEXT")?;
    for id in &ids[..2] {
        let result = client
            .json(
                "/v1/embeddings",
                json!({"model":id,"input":text,"keep_alive":0}),
            )
            .await?;
        let actual = vector(&result)?;
        assert_eq!(result["model"], id.as_str());
        println!(
            "REAL CLI EMBEDDING: {id} -> {} normalized dimensions",
            actual.len()
        );
    }
    let document = client
        .ingest(json!({"source":inline("text/plain", text.as_bytes())}))
        .await?
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(document["status"], "success");
    let result = client.json("/v1/embeddings", json!({"model":ids[0],"input":document["processed_sources"][0]["chunks"],"keep_alive":0})).await?;
    vector(&result)?;
    let (status, body) = client.post("/v1/embeddings", json!({"model":ids[0],"input":[{"type":"image","source":"https://invalid.example/not-fetched.png"}]}), "agent").await?;
    assert_eq!(status, 400, "{body}");
    let mut images = Vec::new();
    let mut vectors = Vec::new();
    for (pixels, expected) in [
        ([255, 0, 0], "red"),
        ([0, 0, 255], "blue"),
        ([0, 0, 255], "blue"),
    ] {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(128, 128, image::Rgb(pixels)))
            .write_to(&mut encoded, image::ImageOutputFormat::Png)?;
        let source = inline("image/png", encoded.get_ref());
        let result = client
            .json(
                "/v1/embeddings",
                json!({"model":ids[2],"input":[{"type":"image","source":source}],"keep_alive":0}),
            )
            .await?;
        vectors.push(vector(&result)?);
        images.push((source, expected));
    }
    assert_ne!(vectors[0], vectors[1]);
    assert_eq!(vectors[1], vectors[2]);
    for (source, expected) in images {
        let answer = client.chat(&ids[3], json!([{"type":"text","text":"What is the main color of this image? Reply with one color word only."}, {"type":"image_url","image_url":{"url":source}}]), false, false).await?;
        assert_eq!(answer.trim().to_ascii_lowercase(), expected, "{answer}");
        println!("REAL CLI VISION: {expected} -> {answer}");
    }
    let recovered = client
        .chat(
            &ids[3],
            json!("What is 2 plus 3? Reply with the digit only."),
            false,
            true,
        )
        .await?;
    assert!(recovered.contains('5'), "{recovered}");
    println!("REAL CLI MEDIA: authenticated GGUF/ONNX vectors, ingest-to-vector, image vectors, repeated vision and text recovery passed");
    Ok(())
}
