//! Real tool installation/task and package controls, using a locally served archive.
#[path = "../../test/cep/wave_b/isolated_home.rs"]
mod isolated_home;
mod support;

use anyhow::{anyhow, Context, Result};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    Json, Router,
};
use cluaiz_cep::{OnDenied, Principal, Scope};
use serde_json::{json, Value};
use std::{
    io::{Cursor, Write},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct RegistryFixture {
    url: String,
    requests: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for RegistryFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn registry_fixture(name: &str, text: &str) -> Result<RegistryFixture> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("SKILL.md", options)?;
    archive.write_all(text.as_bytes())?;
    archive.start_file("package.json", options)?;
    archive.write_all(json!({"name":name,"version":"0.1.0","functions":[
        {"name":"inspect_fixture","description":"Inspect fixture metadata","parameters":{"type":"object"}}
    ]}).to_string().as_bytes())?;
    let bytes = archive.finish()?.into_inner();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    let registry = json!({"routing":{"skills":"skills/index.json"}});
    let family =
        json!({"items":{name:"fixture/package.json","failed-install":"failed/package.json"}});
    let package = json!({"latest_version":"0.1.0","versions":{"0.1.0":{"files":{"file_directory":format!("{url}/bundle.zip")}}}});
    let failed = json!({"latest_version":"0.1.0","versions":{"0.1.0":{"files":{"file_directory":format!("{url}/broken.zip")}}}});
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let router = Router::new().fallback(move |request: Request<Body>| {
        let (registry, family, package, failed, bytes, counter) = (
            registry.clone(),
            family.clone(),
            package.clone(),
            failed.clone(),
            bytes.clone(),
            counter.clone(),
        );
        async move {
            counter.fetch_add(1, Ordering::Relaxed);
            match request.uri().path() {
                "/registry.json" => Json(registry).into_response(),
                "/skills/index.json" => Json(family).into_response(),
                "/skills/fixture/package.json" => Json(package).into_response(),
                "/skills/failed/package.json" => Json(failed).into_response(),
                "/bundle.zip" => (StatusCode::OK, bytes).into_response(),
                "/broken.zip" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                _ => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(RegistryFixture {
        url,
        requests,
        server,
    })
}

async fn ok(router: &Router, method: &str, params: Value) -> Result<Value> {
    support::cep(router, method, params)
        .await?
        .map_err(|e| anyhow!("{method}: {e}"))
}

fn save_json(path: &Path, value: Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn test_tools_install_task_files_settings_cache_execution_and_boundaries() -> Result<()> {
    let home = isolated_home::IsolatedHome::new()?;
    let workspace = tempfile::tempdir()?;
    let name = "cep-fixture-skill";
    let text = format!("---\nname: {name}\ndescription: Inspect fixture metadata\n---\n\nReal local fixture instructions.\n");
    let fixture = registry_fixture(name, &text).await?;
    save_json(
        &home.path().join("engine/config/package.json"),
        json!({"tools":{"manifest_url":format!("{}/registry.json",fixture.url)}}),
    )?;
    for kind in ["plugins", "mcp"] {
        save_json(
            &home
                .path()
                .join(format!("tools/{kind}/cep-{kind}/package.json")),
            json!({"name":format!("cep-{kind}"),"version":"0.1.0","description":"Metadata fixture"}),
        )?;
    }
    let state = Arc::new(cluaiz_api::AppState::new_test_state(
        workspace.path().to_owned(),
    ));
    state.permission_schema.write().await.api_auth.required = true;
    let router = cluaiz_api::routes::build_api_router(state.clone());
    let task = ok(
        &router,
        "tools.install",
        json!({"kind":"skill","name":name}),
    )
    .await?;
    let task_id = task["task"]["task_id"]
        .as_str()
        .context("Install task ID")?;
    let installed = ok(
        &router,
        "tasks.result",
        json!({"task_id":task_id,"wait_ms":30000}),
    )
    .await?;
    assert_eq!(installed["id"], name);
    let status = ok(&router, "tasks.get", json!({"task_id":task_id})).await?;
    assert_eq!(status["status"], "completed");
    let tool_dir = home.path().join("tools/skills").join(name);
    assert_eq!(std::fs::read_to_string(tool_dir.join("SKILL.md"))?, text);
    let listed = ok(&router, "tools.list", json!({"kind":"skill"})).await?;
    assert_eq!(listed.as_array().context("Domain array")?.len(), 1);
    assert_eq!(listed[0]["id"], name);
    assert_eq!(listed[0]["functions"][0]["name"], "inspect_fixture");
    for kind in ["plugin", "mcp"] {
        assert_eq!(
            ok(&router, "tools.list", json!({"kind":kind}))
                .await?
                .as_array()
                .context("Kind list")?
                .len(),
            1
        );
    }
    let discovery = ok(&router, "rpc.discover", json!({"group":"tools"})).await?;
    assert_eq!(
        discovery["methods"].as_array().context("Methods")?.len(),
        13
    );
    let suggested = ok(
        &router,
        "tools.discover",
        json!({"query":format!("use {name}"),"explain":true}),
    )
    .await?;
    assert!(suggested["installed"].as_u64().unwrap_or(0) >= 1);
    assert!(suggested["candidates"].is_array() && suggested["evidence"].is_array());
    let files = ok(&router, "tools.files", json!({"kind":"skill","name":name})).await?;
    assert!(files["files"]
        .as_array()
        .context("Files")?
        .iter()
        .any(|f| f["path"] == "SKILL.md"));
    assert_eq!(
        ok(
            &router,
            "tools.read_file",
            json!({"kind":"skill","name":name,"path":"SKILL.md"})
        )
        .await?["text"],
        text
    );
    let written = ok(
        &router,
        "tools.write_file",
        json!({"kind":"skill","name":name,"text":"Updated UTF-8 नमस्ते"}),
    )
    .await?;
    assert_eq!(written["bytes"], "Updated UTF-8 नमस्ते".len());
    assert_eq!(
        ok(
            &router,
            "tools.call",
            json!({"kind":"skill","name":name,"input":{}})
        )
        .await?,
        "Updated UTF-8 नमस्ते"
    );
    let settings = ok(
        &router,
        "tools.get_settings",
        json!({"kind":"skill","name":name}),
    )
    .await?;
    assert_eq!(settings["values"]["enabled"], true);
    let error = support::cep(
        &router,
        "tools.set_settings",
        json!({"kind":"skill","name":name,
        "patch":{"enabled":false,"security_mode":"invalid"}}),
    )
    .await?
    .unwrap_err();
    assert_eq!(error["code"], -32602);
    assert_eq!(
        ok(
            &router,
            "tools.get_settings",
            json!({"kind":"skill","name":name})
        )
        .await?["values"]["enabled"],
        true
    );
    ok(
        &router,
        "tools.set_settings",
        json!({"kind":"skill","name":name,"patch":{"enabled":false,"execution_mode":"manual"}}),
    )
    .await?;
    assert_eq!(
        support::cep(
            &router,
            "tools.call",
            json!({"kind":"skill","name":name,"input":{}})
        )
        .await?
        .unwrap_err()["code"],
        -32010
    );
    ok(
        &router,
        "tools.set_settings",
        json!({"kind":"skill","name":name,"patch":{"enabled":true}}),
    )
    .await?;
    std::fs::create_dir(tool_dir.join(".cache"))?;
    std::fs::write(tool_dir.join(".cache/temp_a.tmp"), b"123")?;
    std::fs::write(tool_dir.join(".cache/keep.txt"), b"4567")?;
    let cache = ok(&router, "tools.cache", json!({"kind":"skill","name":name})).await?;
    assert_eq!(cache[0]["all_bytes"], 7);
    assert_eq!(cache[0]["temp_bytes"], 3);
    assert_eq!(
        ok(
            &router,
            "tools.clear_cache",
            json!({"kind":"skill","name":name})
        )
        .await?["removed"],
        2
    );
    assert!(tool_dir.join("SKILL.md").exists());
    let requests_before = fixture.requests.load(Ordering::Relaxed);
    for bad in ["../escape", "a/b", "a\\b", "UPPER", "a@1", "", "nul\0name"] {
        for (method, extra) in [
            ("tools.install", json!({})),
            ("tools.remove", json!({})),
            ("tools.files", json!({})),
            ("tools.get_settings", json!({})),
            ("tools.cache", json!({})),
            ("tools.clear_cache", json!({})),
            ("tools.read_file", json!({"path":"SKILL.md"})),
            ("tools.write_file", json!({"text":"blocked"})),
            ("tools.set_settings", json!({"patch":{"enabled":false}})),
            ("tools.call", json!({"input":{}})),
        ] {
            let mut params = json!({"kind":"skill","name":bad});
            params
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert_eq!(
                support::cep(&router, method, params).await?.unwrap_err()["code"],
                -32602,
                "{method}"
            );
        }
    }
    assert_eq!(
        fixture.requests.load(Ordering::Relaxed),
        requests_before,
        "Invalid installs must not reach the registry"
    );
    assert_eq!(
        support::cep(&router, "tools.list", json!({"kind":"model"}))
            .await?
            .unwrap_err()["code"],
        -32602
    );
    for path in ["../outside.txt", "/outside.txt", "a\0b"] {
        assert_eq!(
            support::cep(
                &router,
                "tools.read_file",
                json!({"kind":"skill","name":name,"path":path})
            )
            .await?
            .unwrap_err()["code"],
            -32602
        );
    }
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret.md"), "Outside stays unchanged")?;
    let link = tool_dir.join("outside.md");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(outside.path().join("secret.md"), &link)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.md"), &link)?;
    assert_eq!(
        support::cep(
            &router,
            "tools.read_file",
            json!({"kind":"skill","name":name,"path":"outside.md"})
        )
        .await?
        .unwrap_err()["code"],
        -32602
    );
    let files = ok(&router, "tools.files", json!({"kind":"skill","name":name})).await?;
    assert_eq!(files["truncated"], true);
    assert_eq!(files["skipped"], 1);
    let principal = Arc::new(Principal {
        id: "token:tool-contract".into(),
        scopes: vec![Scope::Read, Scope::Write, Scope::Exec],
        roots: Some(vec![workspace.path().canonicalize()?]),
        on_denied: OnDenied::Error,
        owner: false,
    });
    for (method, params, needs) in [
        (
            "tools.read_file",
            json!({"kind":"skill","name":name,"path":"SKILL.md"}),
            "roots",
        ),
        (
            "tools.call",
            json!({"kind":"skill","name":name,"input":{}}),
            "roots",
        ),
        (
            "tools.open_file",
            json!({"kind":"skill","name":name}),
            "owner",
        ),
    ] {
        let request =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))?;
        let reply = state
            .cep
            .call(
                cluaiz_api::cep::CallCtx {
                    state: state.clone(),
                },
                principal.clone(),
                &request,
            )
            .await
            .context("Scoped reply")?;
        let reply: Value = serde_json::from_slice(&reply)?;
        assert_eq!(reply["error"]["code"], -32002);
        assert_eq!(reply["error"]["data"]["needs"], needs);
    }
    let task = ok(
        &router,
        "tools.install",
        json!({"kind":"skill","name":"failed-install"}),
    )
    .await?;
    assert_eq!(
        support::cep(
            &router,
            "tasks.result",
            json!({"task_id":task["task"]["task_id"],"wait_ms":30000})
        )
        .await?
        .unwrap_err()["code"],
        -32010
    );
    assert!(!home.path().join("tools/skills/failed-install").exists());
    std::fs::remove_dir(tool_dir.join(".cache"))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(outside.path(), tool_dir.join(".cache"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), tool_dir.join(".cache"))?;
    assert_eq!(
        support::cep(
            &router,
            "tools.clear_cache",
            json!({"kind":"skill","name":name})
        )
        .await?
        .unwrap_err()["code"],
        -32602
    );
    ok(&router, "tools.remove", json!({"kind":"skill","name":name})).await?;
    assert!(!tool_dir.exists());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.md"))?,
        "Outside stays unchanged"
    );
    assert!(ok(&router, "tools.list", json!({"kind":"skill"}))
        .await?
        .as_array()
        .context("Final list")?
        .is_empty());
    Ok(())
}
