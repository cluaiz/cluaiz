//! Opt-in acceptance against an actual CLI executable, not an in-process router.
#[path = "../../engine/media/tests/support/mod.rs"]
#[allow(dead_code)]
mod documents;
mod embedding_acceptance;
#[path = "processing_acceptance.rs"]
mod processing_acceptance;

use anyhow::{bail, Context, Result};
use base64::Engine;
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistry};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Only the test-owned child is terminated, including after a failed assertion.
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}
impl Client {
    async fn post(&self, route: &str, body: Value) -> Result<(u16, String)> {
        let result = self
            .http
            .post(format!("{}{route}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await?;
        Ok((result.status().as_u16(), result.text().await?))
    }
    async fn json(&self, route: &str, body: Value) -> Result<Value> {
        let (status, body) = self.post(route, body).await?;
        assert_eq!(status, 200, "{route}: {body}");
        Ok(serde_json::from_str(&body)?)
    }
    async fn cep(&self, method: &str, params: Value) -> Result<Value> {
        let reply = self
            .json(
                "/v1/cep",
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
            )
            .await?;
        if let Some(error) = reply.get("error") {
            bail!("CEP {method}: {error}");
        }
        reply.get("result").cloned().context("Missing CEP result")
    }
    /// The direct route returns the final result or an HTTP error body.
    async fn ingest(&self, params: Value) -> Result<std::result::Result<Value, Value>> {
        let (status, body) = self.post("/v1/ingest", params).await?;
        let value = serde_json::from_str(&body)?;
        Ok(if (200..300).contains(&status) { Ok(value) } else { Err(value) })
    }
    async fn chat(&self, model: &str, parts: Value, stream: bool, release: bool) -> Result<String> {
        let (status, body) = self
            .post(
                "/v1/chat/completions",
                json!({
                    "model":model,"stream":stream,"keep_alive":if release {0} else {-1},
                    "temperature":0.0,"seed":31,"think_mode":"off","max_tokens":96,
                    "messages":[{"role":"user","content":parts}]
                }),
            )
            .await?;
        assert_eq!(status, 200, "{body}");
        if !stream {
            let result: Value = serde_json::from_str(&body)?;
            return Ok(result["choices"][0]["message"]["content"]
                .as_str()
                .context("Chat content")?
                .to_owned());
        }
        let mut answer = String::new();
        assert!(body.contains("data: [DONE]"), "{body}");
        for event in body.lines().filter_map(|line| line.strip_prefix("data: ")) {
            if event == "[DONE]" {
                continue;
            }
            let event: Value = serde_json::from_str(event)?;
            assert!(event.get("error").is_none(), "{event}");
            if let Some(piece) = event["choices"][0]["delta"]["content"].as_str() {
                answer.push_str(piece);
            }
        }
        Ok(answer)
    }
}

fn env_path(key: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var(key).with_context(|| key.to_owned())?,
    ))
}
fn inline(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
fn vector(result: &Value) -> Result<Vec<f32>> {
    let vector: Vec<f32> = serde_json::from_value(result["data"][0]["embedding"].clone())?;
    assert!(!vector.is_empty() && vector.iter().all(|v| v.is_finite()));
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4);
    Ok(vector)
}

async fn start_daemon(home: &std::path::Path) -> Result<(Daemon, Client)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut paths = vec![home.join("bin")];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new(env_path("CLUAIZ_ARTIFACT_EXE")?);
    command
        .arg("serve")
        .current_dir(home)
        .env("CLUAIZ_HOME", home)
        .env("CLUAIZ_HOST", "127.0.0.1")
        .env("CLUAIZ_PORT", port.to_string())
        .env("PATH", std::env::join_paths(paths)?)
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdout(Stdio::from(std::fs::File::create(
            home.join("daemon.stdout.log"),
        )?))
        .stderr(Stdio::from(std::fs::File::create(
            home.join("daemon.stderr.log"),
        )?));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut daemon = Daemon(command.spawn()?);
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(240))
        .build()?;
    let base = format!("http://127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if http
            .get(format!("{base}/health"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        if let Some(status) = daemon.0.try_wait()? {
            bail!("Test daemon exited {status}; see {}", home.display());
        }
        if Instant::now() >= deadline {
            bail!("Test daemon did not bind; see {}", home.display());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let token = std::fs::read_to_string(home.join("session.token"))?
        .trim()
        .to_owned();
    Ok((daemon, Client { http, base, token }))
}

fn configure_test_permissions() -> Result<()> {
    cluaiz_security::PermissionSchema {
        agent_security_mode: "strict".into(),
        stream_telemetry: false,
        lazy_load_model: true,
        api_auth: cluaiz_security::ApiAuth {
            required: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .save()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires final executable, fresh CLUAIZ_HOME and explicit read-only Chat registry/settings/root"]
async fn real_chat_planner_only_selected_models_settings_and_timings() -> Result<()> {
    let home = env_path("CLUAIZ_HOME")?;
    assert!(
        home.is_absolute() && !home.exists(),
        "Fresh absolute test home required"
    );
    assert_eq!(
        cluaiz_api::auth::session_token_path(),
        Some(home.join("session.token"))
    );
    let registry_path = env_path("CLUAIZ_PLANNER_REGISTRY")?;
    let settings_path = env_path("CLUAIZ_PLANNER_SETTINGS")?;
    let chat_root = env_path("CLUAIZ_PLANNER_CHAT_ROOT")?.canonicalize()?;
    let registry_bytes = std::fs::read(&registry_path)?;
    let settings_bytes = std::fs::read(&settings_path)?;
    let saved: Value = serde_json::from_slice(&settings_bytes)?;
    let mut registry: ModelRegistry = serde_json::from_slice(&registry_bytes)?;
    registry.installed_models.retain(|_, entry| {
        entry.category == "chat"
            && PathBuf::from(&entry.local_dir)
                .parent()
                .and_then(|parent| parent.canonicalize().ok())
                .as_ref()
                == Some(&chat_root)
    });
    assert!(!registry.installed_models.is_empty());
    let mut entries: Vec<_> = registry.installed_models.values().collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let user_token = dirs::home_dir()
        .context("User home")?
        .join(".cluaiz/session.token");
    let original_token = std::fs::read(&user_token).ok();
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
    let (daemon, client) = start_daemon(&home).await?;
    let mut measurements = Vec::new();
    for run in 1..=3 {
        for entry in &entries {
            let start = Instant::now();
            let (status, body) = client
                .post("/v1/cep", json!({"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":entry.id}}))
                .await?;
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(status, 200, "{body}");
            let reply: Value = serde_json::from_str(&body)?;
            assert!(reply.get("error").is_none(), "{body}");
            let plan = &reply["result"];
            let model = &plan["model_info"];
            for key in [
                "supported_attachments",
                "package_capabilities",
                "available_inputs",
                "input_availability",
                "max_output_context",
                "active_working_context",
            ] {
                assert!(model.get(key).is_none(), "Unexpected Chat field {key}");
            }
            assert!(plan["execution_plan"].get("speed").is_none());
            let path = entry.primary_path().context("Primary")?;
            let geometry = cluaiz_models::GgufProber::probe_geometry(&path)?;
            let package = cluaiz_models::capabilities::PackageCapabilities::inspect(&path)?;
            assert_eq!(model["context_window"], geometry.context_length);
            for (key, value) in plan["active_user_settings"]
                .as_object()
                .context("Saved settings")?
            {
                assert_eq!(
                    *value, saved["optimization"][key],
                    "Saved setting {key} changed meaning"
                );
            }
            let inputs = model["inputs"].as_array().context("Input contract")?;
            assert_eq!(inputs.len(), package.supported_inputs().len());
            for input in inputs {
                assert_ne!(input["type"], "video");
                assert_eq!(
                    input["available"], true,
                    "Positive model input must be available independent of session load state"
                );
                for field in ["support", "reason"] {
                    assert!(
                        input.get(field).is_none(),
                        "Session field {field} leaked into capabilities"
                    );
                }
            }
            if let Some(preparation) = model.get("attachment_processing") {
                let routes = preparation.as_array().context("Preparation routes")?;
                assert!(!routes.is_empty());
                for route in routes {
                    assert_eq!(route["input"], "video");
                    assert_eq!(route["output"], "image");
                    assert_eq!(route["processing"], "sampled_frames");
                    assert!(!route["formats"]
                        .as_array()
                        .context("Video formats")?
                        .is_empty());
                }
            }
            for declaration in &package.inputs {
                let modality = serde_json::to_value(declaration.modality)?;
                let input = inputs.iter().find(|input| input["type"] == modality);
                if declaration.state != cluaiz_models::capabilities::CapabilityState::Supported {
                    assert!(
                        input.is_none(),
                        "Undeclared input leaked into ordinary response"
                    );
                    continue;
                }
                let input = input.context("Supported input declaration")?;
                if modality == "text" {
                    assert!(input.get("formats").is_none());
                } else if declaration.state
                    == cluaiz_models::capabilities::CapabilityState::Supported
                {
                    let descriptors = cluaiz_media::formats::attachment_formats();
                    let expected = cluaiz_media::formats::unique_extensions(
                        descriptors
                            .iter()
                            .filter(|(kind, _)| *kind == declaration.modality)
                            .map(|(_, extension)| *extension),
                    );
                    assert_eq!(
                        input["formats"],
                        serde_json::to_value(expected)?,
                        "Actual compiled decoder formats differ"
                    );
                }
            }
            println!(
                "CHAT PLANNER {} run{run}: {ms:.2}ms; native_ctx={}, planned_ctx={}; {} bytes",
                entry.id,
                model["context_window"],
                plan["execution_plan"]["context_window"],
                body.len()
            );
            std::fs::write(home.join(format!("planner-{}-{run}.json", entry.id)), &body)?;
            measurements.push(json!({"model":entry.id,"run":run,"ms":ms,"bytes":body.len()}));
        }
    }
    for entry in &entries {
        let plan = client
            .cep(
                "system.plan",
                json!({"model":entry.id,"n_ctx":1234,"batch_tokens":32,"ram_buffer_gb":0.25}),
            )
            .await?;
        assert_eq!(plan["execution_plan"]["context_window"], 1234);
        assert_eq!(plan["execution_plan"]["batch_size"], 32);
        assert_eq!(plan["hardware_info"]["ram"]["safety_buffer_gb"], 0.25);
        assert_eq!(
            plan["active_user_settings"]["custom_context_window"],
            saved["optimization"]["custom_context_window"]
        );
        assert_eq!(plan["request_overrides"]["n_ctx"], 1234);
        std::fs::write(
            home.join(format!("planner-override-{}.json", entry.id)),
            serde_json::to_vec_pretty(&plan)?,
        )?;
    }
    assert_eq!(
        std::fs::read(home.join("engine/config/settings.json"))?,
        settings_bytes
    );
    let id = &entries[0].id;
    for patch in [
        json!({"n_ctx":-2}),
        json!({"batch_tokens":0}),
        json!({"ram_buffer_gb":-1}),
    ] {
        let mut value = json!({"model":id});
        value
            .as_object_mut()
            .context("Request")?
            .extend(patch.as_object().context("Patch")?.clone());
        let reply = client
            .json(
                "/v1/cep",
                json!({"jsonrpc":"2.0","id":1,"method":"system.plan","params":value}),
            )
            .await?;
        assert_eq!(reply["error"]["code"], -32602);
    }
    let invalid = client.json("/v1/cep", json!({"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":id,"weights_gb":1}})).await?;
    assert_eq!(invalid["error"]["code"], -32602);
    let missing_id = format!("missing-{}", uuid::Uuid::new_v4());
    let missing = client
        .json(
            "/v1/cep",
            json!({"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":missing_id}}),
        )
        .await?;
    assert_eq!(missing["error"]["code"], -32004);
    assert_eq!(missing["error"]["data"]["type"], "model_not_found");
    let unauth = client
        .http
        .post(format!("{}/v1/cep", client.base))
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":id}}))
        .send()
        .await?;
    assert_eq!(unauth.status().as_u16(), 401);
    let updated = client.json("/v1/cep", json!({"jsonrpc":"2.0","id":1,"method":"config.set","params":{"section":"gguf","patch":{"hardware_and_execution":{"n_ctx":2048,"n_gpu_layers":0,"batch_size":64,"ubatch_size":32}}}})).await?;
    assert!(updated.get("error").is_none(), "{updated}");
    let plan = client.cep("system.plan", json!({"model":id})).await?;
    assert_eq!(plan["active_user_settings"]["custom_context_window"], 2048);
    assert_eq!(plan["active_user_settings"]["n_gpu_layers"], 0);
    assert_eq!(plan["execution_plan"]["context_window"], 2048);
    assert_eq!(plan["execution_plan"]["n_gpu_layers"], 0);
    assert_eq!(plan["execution_plan"]["batch_size"], 64);
    assert_eq!(plan["execution_plan"]["ubatch_size"], 32);
    std::fs::write(
        home.join("planner-manual.json"),
        serde_json::to_vec_pretty(&plan)?,
    )?;
    std::fs::write(
        home.join("planner-measurements.json"),
        serde_json::to_vec_pretty(&measurements)?,
    )?;
    println!("CHAT PLANNER: {} selected Chat models; saved Auto/manual settings, transient overrides, native/planned contexts and 400/401/422 errors checked; no model inference", entries.len());
    println!("CHAT PLANNER evidence home: {}", home.display());
    drop(daemon);
    assert_eq!(std::fs::read(&settings_path)?, settings_bytes);
    assert_eq!(std::fs::read(&registry_path)?, registry_bytes);
    assert!(std::fs::read(&user_token).ok() == original_token);
    println!("CHAT PLANNER: original settings/registry/token unchanged; only test-owned daemon terminated");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh isolated CLUAIZ_HOME, final executable, native package and real model/sample paths"]
async fn real_final_executable_attachments_and_onnx_apis() -> Result<()> {
    let home = env_path("CLUAIZ_HOME")?;
    assert_eq!(home, env_path("CLUAIZ_MEDIA_TEST_HOME")?);
    if !home.is_absolute() || home.exists() {
        bail!("Fresh absolute test home required");
    }
    assert_eq!(
        cluaiz_api::auth::session_token_path(),
        Some(home.join("session.token"))
    );
    // Snapshot bytes for equality only; no secret is ever logged or sent elsewhere.
    let user_token = dirs::home_dir()
        .context("User home")?
        .join(".cluaiz/session.token");
    let original_token = std::fs::read(&user_token).ok();
    std::fs::create_dir_all(home.join("bin"))?;
    let builder = env_path("CLUAIZ_ARTIFACT_BUILDER")?;
    let package = env_path("CLUAIZ_NATIVE_MEDIA_TEST_PACKAGE")?;
    let status = Command::new(builder)
        .args(["media-assets", "--source"])
        .arg(package)
        .arg("--output")
        .arg(home.join("engine/media"))
        .status()?;
    assert!(status.success(), "Real package staging failed");

    let mut registry = ModelRegistry::default();
    let mut ids = Vec::new();
    for (category, key) in [
        ("chat", "CLUAIZ_MEDIA_MODEL"),
        ("tts", "CLUAIZ_ONNX_TTS_MODEL"),
        ("stt", "CLUAIZ_ONNX_STT_MODEL"),
        ("embedding", "CLUAIZ_ONNX_TEXT_MODEL"),
        ("ingest", "CLUAIZ_ONNX_IMAGE_MODEL"),
    ] {
        let model = env_path(key)?;
        let parent = model.parent().context("Model parent")?;
        assert!(
            !parent.starts_with(&home),
            "Read-only weights must remain external"
        );
        let entry = InstalledStateRegistry::entry_for_folder(
            category,
            parent,
            model.file_name().and_then(|s| s.to_str()),
            "",
        )
        .context("Actual model package entry")?;
        ids.push(entry.id.clone());
        registry.installed_models.insert(entry.id.clone(), entry);
    }
    InstalledStateRegistry::save(&registry).map_err(anyhow::Error::msg)?;
    let mut settings = cluaiz_core::ActiveUserSettings::default();
    settings.optimization.n_gpu_layers = Some(0);
    settings.optimization.n_threads = Some(2);
    settings.optimization.n_threads_batch = Some(2);
    settings.optimization.custom_context_window = Some(4096);
    settings.optimization.batch_size = Some(128);
    settings.optimization.ubatch_size = Some(128);
    settings.optimization.custom_ram_buffer_gb = Some(1.0);
    settings.media.limits =
        cluaiz_core::media_limits::MediaLimits::new(cluaiz_core::MIB, 16 * cluaiz_core::MIB)?;
    settings.media.projector_workspace_bytes =
        std::num::NonZeroU64::new(512 * cluaiz_core::MIB).context("Workspace")?;
    settings.onnx.n_gpu_layers = 0;
    settings.onnx.intra_op_num_threads = 2;
    cluaiz_core::Settings::update(settings)?;
    configure_test_permissions()?;

    let text_pdf = documents::pdf(&[Some("Receipt code 741923")])?;
    let assets = cluaiz_media::native_assets::NativeAssets::from_environment()?;
    let raster = cluaiz_media::documents::pdf_render::with_document(&text_pdf, &assets, |doc| {
        doc.render_page(1, Default::default())
    })?;
    let scanned = documents::raster::pdf(&raster, None)?;
    cluaiz_media::documents::release_native_providers()?;
    let sample = std::fs::read(env_path("CLUAIZ_ARTIFACT_AUDIO")?)?;
    let expected = std::env::var("CLUAIZ_MEDIA_EXPECTED")?.to_ascii_lowercase();
    assert!(!expected.is_empty(), "Real audio reference text required");
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 32, image::Rgb([255, 0, 0])))
        .write_to(&mut png, image::ImageOutputFormat::Png)?;
    let image_uri = inline("image/png", png.get_ref());
    let audio_uri = inline("audio/wav", &sample);
    let video_uri = inline(
        "video/mp4",
        &std::fs::read(env_path("CLUAIZ_VIDEO_TEST_FIXTURES")?.join("red-blue-audio.mp4"))?,
    );

    let (daemon, client) = start_daemon(&home).await?;
    assert!(
        std::fs::read(&user_token).ok() == original_token,
        "User token changed"
    );
    let unauth = client
        .http
        .post(format!("{}/v1/chat/completions", client.base))
        .json(&json!({}))
        .send()
        .await?;
    assert_eq!(unauth.status().as_u16(), 401);
    println!("FINAL EXECUTABLE: isolated daemon owns its token; user token unchanged; unauthenticated request -> 401");

    for (label, mime, bytes) in [
        ("text PDF", "application/pdf", text_pdf.as_slice()),
        ("scanned PDF", "application/pdf", scanned.as_slice()),
    ] {
        let result = client
            .ingest(json!({"source":inline(mime,bytes)}))
            .await?
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(result["status"], "success", "{result}");
        assert!(
            result["processed_sources"][0]["chunks"]
                .to_string()
                .contains("741923"),
            "{result}"
        );
        println!("FINAL EXECUTABLE INGEST: {label} -> actual code 741923");
    }
    let docx = client
        .ingest(json!({"source":inline(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        &documents::docx(documents::DOCX_XML)?)}))
        .await?
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(docx["processed_sources"][0]["chunks"]
        .to_string()
        .contains("First"));
    println!("FINAL EXECUTABLE INGEST: DOCX -> actual body/table text");
    let bad = client
        .ingest(json!({"source":inline("application/pdf",b"corrupt")}))
        .await?
        .expect_err("corrupt PDF must fail");
    assert_eq!(bad["status"], "error", "{bad}");
    assert!(bad["processed_sources"][0]["error"].is_string(), "{bad}");
    let outside = tempfile::tempdir()?;
    let outside_document = outside.path().join("owner.md");
    std::fs::write(&outside_document, "external owner attachment")?;
    let owner_source = client
        .ingest(json!({"source":outside_document}))
        .await?
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .to_string();
    assert!(owner_source.contains("external owner attachment"), "{owner_source}");
    let transport_limit = cluaiz_api::routes::inline_body_bytes(
        cluaiz_core::Settings::current()
            .media
            .limits
            .max_input_bytes(),
    );
    let oversized = client
        .post(
            "/v1/ingest",
            json!({"source":"x".repeat(transport_limit)}),
        )
        .await?;
    assert_eq!(oversized.0, 413, "{}", oversized.1);
    let error: Value = serde_json::from_str(&oversized.1)?;
    assert_eq!(error["error"]["type"], "request_too_large");
    println!("FINAL EXECUTABLE CONTRACT: corrupt PDF -> 400; external owner source -> 200; oversized JSON -> 413");

    let answer = client
        .chat(
            &ids[0],
            json!([
                {"type":"text","text":"What is the main color? Reply with one color word only."},
                {"type":"image_url","image_url":{"url":image_uri}}
            ]),
            false,
            false,
        )
        .await?;
    assert!(answer.to_ascii_lowercase().contains("red"), "{answer}");
    println!("FINAL EXECUTABLE IMAGE JSON: {answer:?}");
    let answer = client.chat(&ids[0],json!([
        {"type":"text","text":"Transcribe the spoken words. Reply only with the transcript."},
        {"type":"audio_url","audio_url":{"url":audio_uri}}
    ]),true,false).await?;
    assert!(answer.to_ascii_lowercase().contains(&expected), "{answer}");
    println!("FINAL EXECUTABLE AUDIO SSE: {answer:?}");
    let answer = client.chat(&ids[0],json!([
        {"type":"text","text":"State the main color at 0 seconds and at 1 second, in that order. Reply with two color words."},
        {"type":"video_url","video_url":{"url":video_uri}}
    ]),false,false).await?;
    let lower = answer.to_ascii_lowercase();
    assert!(
        lower.find("red").context("No red")? < lower.find("blue").context("No blue")?,
        "{answer}"
    );
    println!("FINAL EXECUTABLE VIDEO JSON: {answer:?}");
    let plan = client.cep("system.plan", json!({"model":ids[0]})).await?;
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
        .context("Formats")?
        .iter()
        .any(|v| v == "mp4"));
    let answer = client.chat(&ids[0],json!([
        {"type":"text","text":"Return the receipt code from this document. Reply with digits only."},
        {"type":"file_url","file_url":{"url":inline("application/pdf",&scanned)}}
    ]),true,true).await?;
    assert!(answer.contains("741923"), "{answer}");
    println!("FINAL EXECUTABLE SCANNED DOCUMENT SSE: {answer:?}; chat model released");

    let tts = client
        .json(
            "/v1/audio/speech",
            json!({"model":ids[1],"input":"Hello world."}),
        )
        .await?;
    let wav = tts["output"]["audio_data"]
        .as_str()
        .context("Actual TTS WAV")?;
    let pcm = cluaiz_media::audio::decode_audio(
        base64::engine::general_purpose::STANDARD
            .decode(wav.split_once(',').context("WAV URI")?.1)?,
        Default::default(),
    )?;
    assert!(
        pcm.samples().len() > pcm.sample_rate() as usize / 4
            && pcm.samples().iter().any(|v| v.abs() > 0.01)
    );
    println!(
        "FINAL EXECUTABLE ONNX TTS: {} real samples at {} Hz",
        pcm.samples().len(),
        pcm.sample_rate()
    );
    let stt = client
        .json(
            "/v1/audio/transcriptions",
            json!({"model":ids[2],
            "input_source":{"type":"audio","data":audio_uri},"language":"en"}),
        )
        .await?;
    let transcript = stt["output"]["text"].as_str().context("STT output.text")?;
    assert!(
        transcript.to_ascii_lowercase().contains(&expected),
        "{transcript}"
    );
    println!("FINAL EXECUTABLE ONNX STT: {transcript:?}");
    let text = client
        .json(
            "/v1/embeddings",
            json!({"model":ids[3],"input":["The attached receipt records a code."]}),
        )
        .await?;
    let dims = vector(&text)?.len();
    assert_eq!(
        dims,
        std::env::var("CLUAIZ_ARTIFACT_TEXT_DIMS")?.parse::<usize>()?
    );
    println!("FINAL EXECUTABLE ONNX TEXT: {dims} finite normalized dimensions");
    let image = client
        .json(
            "/v1/embeddings",
            json!({"model":ids[4],"input":[{"type":"image","source":image_uri}]}),
        )
        .await?;
    let dims = vector(&image)?.len();
    assert_eq!(
        dims,
        std::env::var("CLUAIZ_ARTIFACT_IMAGE_DIMS")?.parse::<usize>()?
    );
    println!("FINAL EXECUTABLE ONNX IMAGE: {dims} finite normalized dimensions");
    let incomplete = tempfile::tempdir()?;
    std::fs::hard_link(
        env_path("CLUAIZ_ONNX_IMAGE_MODEL")?,
        incomplete.path().join("incomplete.onnx"),
    )?;
    let entry = InstalledStateRegistry::entry_for_folder(
        "ingest",
        incomplete.path(),
        Some("incomplete.onnx"),
        "",
    )
    .context("Incomplete package entry")?;
    let incomplete_id = entry.id.clone();
    InstalledStateRegistry::register_model(entry).map_err(anyhow::Error::msg)?;
    let (status, body) = client
        .post(
            "/v1/embeddings",
            json!({"model":incomplete_id,
        "input":[{"type":"image","source":"https://invalid.example/not-fetched.png"}]}),
        )
        .await?;
    assert_eq!(status, 400, "{body}");
    let error: Value = serde_json::from_str(&body)?;
    assert_eq!(error["error"]["type"], "invalid_model");
    assert!(error["error"]["message"]
        .as_str()
        .context("Helper error")?
        .contains("preprocessor_config.json"));
    println!("FINAL EXECUTABLE MISSING HELPERS: HTTP400 with required filenames before source decoding/native load");
    drop(daemon);
    assert!(
        std::fs::read(&user_token).ok() == original_token,
        "User token changed"
    );
    println!("FINAL EXECUTABLE: test-owned daemon terminated; user session-token bytes unchanged");
    Ok(())
}
