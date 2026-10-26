//! Selected-model proof using the real executable in a private home, without modifying its planner.
use super::*;
use futures::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh CLUAIZ_HOME, executable, explicit registry/settings/model ID and optional image"]
async fn real_repeated_attachments_report_progress_and_keep_loaded_plan() -> Result<()> {
    let home = env_path("CLUAIZ_HOME")?;
    assert!(
        home.is_absolute() && !home.exists(),
        "Fresh test home required"
    );
    let registry_path = env_path("CLUAIZ_PLANNER_REGISTRY")?;
    let settings_path = env_path("CLUAIZ_PLANNER_SETTINGS")?;
    let model_id = std::env::var("CLUAIZ_MEDIA_MODEL_ID")?;
    let registry_bytes = std::fs::read(&registry_path)?;
    let settings_bytes = std::fs::read(&settings_path)?;
    let mut registry: ModelRegistry = serde_json::from_slice(&registry_bytes)?;
    registry
        .installed_models
        .retain(|id, entry| id == &model_id && entry.category == "chat");
    assert_eq!(registry.installed_models.len(), 1);
    std::fs::create_dir_all(home.join("engine/config"))?;
    std::fs::write(
        home.join("engine/config/model_registry.json"),
        serde_json::to_vec(&registry)?,
    )?;
    std::fs::write(home.join("engine/config/settings.json"), &settings_bytes)?;
    let profile = settings_path.with_file_name("hardware_profile.json");
    if profile.is_file() {
        std::fs::copy(profile, home.join("engine/config/hardware_profile.json"))?;
    }
    configure_test_permissions()?;
    let (_daemon, client) = start_daemon(&home).await?;
    let mut fixtures = Vec::new();
    for (size, color, word) in [(128, [255, 0, 0], "red"), (256, [0, 0, 255], "blue")] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(size, size, image::Rgb(color)))
            .write_to(&mut bytes, image::ImageOutputFormat::Png)?;
        fixtures.push((inline("image/png", bytes.get_ref()), Some(word)));
    }
    if let Ok(path) = std::env::var("CLUAIZ_PROGRESS_IMAGE") {
        assert!(std::path::Path::new(&path).is_absolute());
        fixtures.push((path.clone(), None));
        fixtures.push((path, None));
    }
    let mut load = None;
    let mut measurements = Vec::new();
    for (run, (source, expected)) in fixtures.into_iter().enumerate() {
        let started = Instant::now();
        let response = client.http.post(format!("{}/v1/chat/completions", client.base)).bearer_auth(&client.token)
            .json(&json!({
                "model":model_id,"stream":true,"return_progress":true,"keep_alive":-1,
                "temperature":0.0,"seed":31,"think_mode":"off","max_tokens":32,"temporary_chat":"strict",
                "messages":[{"role":"user","content":[{"type":"text","text":if expected.is_some() {
                    "What is the main color of the image? Reply with one color word only."
                } else { "Describe this image briefly." }}, {"type":"image_url","image_url":{"url":source}}]}]
            })).send().await?;
        assert_eq!(response.status(), 200);
        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        let mut text = String::new();
        let mut events = Vec::new();
        let mut first_progress_ms = None;
        let mut first_token_ms = None;
        let mut id = None;
        let mut done = false;
        while let Some(bytes) = stream.next().await {
            pending.extend_from_slice(&bytes?);
            while let Some(end) = pending.windows(2).position(|w| w == b"\n\n") {
                let frame = String::from_utf8(pending.drain(..end + 2).collect())?;
                let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) else {
                    continue;
                };
                if data == "[DONE]" {
                    done = true;
                    continue;
                }
                let event: Value = serde_json::from_str(data)?;
                assert!(event.get("error").is_none(), "run {run}: {event}");
                let event_id = event["id"].as_str().context("Event ID")?.to_owned();
                if let Some(id) = &id {
                    assert_eq!(id, &event_id);
                } else {
                    id = Some(event_id);
                }
                if frame.starts_with("event: cluaiz.progress") {
                    assert!(!done, "Progress after DONE");
                    first_progress_ms.get_or_insert(started.elapsed().as_millis());
                    events.push(event["processing"].clone());
                } else if let Some(piece) = event["choices"][0]["delta"]["content"].as_str() {
                    first_token_ms.get_or_insert(started.elapsed().as_millis());
                    assert!(events
                        .iter()
                        .any(|e| e["stage"] == "prefill" && e["state"] == "completed"));
                    text.push_str(piece);
                }
                if !event["engine"]["load"].is_null() {
                    if let Some(load) = &load {
                        assert_eq!(load, &event["engine"]["load"]);
                    } else {
                        load = Some(event["engine"]["load"].clone());
                    }
                }
            }
        }
        assert!(done && !text.trim().is_empty());
        if let Some(expected) = expected {
            assert_eq!(text.trim().trim_matches('.').to_ascii_lowercase(), expected);
        }
        for stage in ["reading", "decoding", "encoding", "prefill"] {
            assert!(
                events
                    .iter()
                    .any(|e| e["stage"] == stage && e["state"] == "completed"),
                "{events:?}"
            );
        }
        assert!(events
            .iter()
            .filter(|e| e["stage"] == "encoding")
            .all(|e| e["attachment"]["message_index"] == 0 && e["attachment"]["part_index"] == 1));
        assert_eq!(
            events
                .iter()
                .filter(|e| e["stage"] == "request" && e["state"] == "completed")
                .count(),
            1
        );
        if run > 0 {
            assert!(events
                .iter()
                .any(|e| e["stage"] == "projector_loading" && e["state"] == "reused"));
        }
        let first = first_progress_ms.context("Missing early progress")?;
        let ttft = first_token_ms.context("Missing answer token")?;
        assert!(first < ttft);
        std::fs::write(
            home.join(format!("processing-events-run-{}.json", run + 1)),
            serde_json::to_vec_pretty(&events)?,
        )?;
        let measured = json!({"run":run+1,"progress_events":events.len(),"first_progress_ms":first,
            "first_content_ms":ttft,"elapsed_ms":started.elapsed().as_millis(),"answer_present":true});
        eprintln!("ATTACHMENT PROOF {measured}");
        measurements.push(measured);
        assert!(client
            .http
            .get(format!("{}/health", client.base))
            .send()
            .await?
            .status()
            .is_success());
    }
    assert!(load.is_some());
    assert_eq!(std::fs::read(&registry_path)?, registry_bytes);
    assert_eq!(std::fs::read(&settings_path)?, settings_bytes);
    std::fs::write(
        home.join("processing-proof.json"),
        serde_json::to_vec_pretty(&json!({
            "model":model_id,"runs":measurements,"loaded_plan_unchanged":true,"load":load,
            "original_registry_unchanged":true,"original_settings_unchanged":true
        }))?,
    )?;
    Ok(())
}
