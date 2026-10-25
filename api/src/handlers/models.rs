//! Standard OpenAI/Ollama model routes. CEP controls live in `cep::methods::models`.

use crate::AppState;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{extract::State, Json};
use cluaiz_models::fetcher::Pulled;
use cluaiz_models::registry::InstalledStateRegistry;
use cluaiz_models::DownloadEvent;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::mpsc;

/// GET /v1/models (OpenAI Standard Format)
pub async fn v1_models(State(state): State<Arc<AppState>>) -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let now = chrono::Utc::now().timestamp();
    let mut data = Vec::new();

    for (id, entry) in registry.installed_models {
        let task_support =
            if entry.format_type == "onnx" {
                if let Some(package) = entry.metadata.capabilities.as_ref() {
                    let loaded = state.host.onnx.readiness(&entry).await.ok().flatten();
                    let formats = cluaiz_media::formats::attachment_formats();
                    let routes: Vec<_> =
                        formats
                            .iter()
                            .map(|&(input, format)| {
                                cluaiz_runtime::media_support::AttachmentRoute { format, input }
                            })
                            .collect();
                    Some(cluaiz_runtime::media_support::resolve_onnx_support(
                        package,
                        loaded.as_ref(),
                        &routes,
                    ))
                } else {
                    None
                }
            } else {
                None
            };
        data.push(json!({
            "id": id,
            "object": "model",
            "created": now,
            "owned_by": "cluaiz",
            "permission": [],
            "root": id,
            "parent": null,
            "category": entry.category,
            "context_window": entry.metadata.context_window,
            "package_capabilities": entry.metadata.capabilities,
            "task_support": task_support
        }));
    }

    Json(json!({
        "object": "list",
        "data": data
    }))
}

/// GET /api/tags (Ollama Compatibility)
pub async fn tags() -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let tags: Vec<Value> = registry
        .installed_models
        .values()
        .map(|entry| {
            let total_size = entry.files.iter().map(|f| f.size_bytes).sum::<u64>();
            json!({
                "name": entry.id,
                "model": entry.id,
                "modified_at": "2026-10-08T00:00:00Z",
                "size": total_size,
                "details": {
                    "format": entry.format_type,
                    "family": entry.metadata.architecture,
                    "parameter_size": entry.metadata.parameters
                }
            })
        })
        .collect();

    Json(json!({ "models": tags }))
}

/// Ollama's `/api/pull` body. `insecure` is accepted and has no effect (downloads use HTTPS).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullPayload {
    pub model: String,
    #[serde(default)]
    pub insecure: bool,
    /// One NDJSON line per step (Ollama's default), else one answer at the end
    #[serde(default = "stream_by_default")]
    pub stream: bool,
}

fn stream_by_default() -> bool {
    true
}

/// POST /api/pull (Ollama): downloads and registers the model with the same code as `cluaiz pull`.
/// Streams `{"status"}` lines (with `total` / `completed` while bytes come in) and ends with
/// `{"status":"success"}` or `{"error"}`; closing the connection stops the download.
pub async fn pull_model(Json(payload): Json<PullPayload>) -> Response {
    let model = payload.model.trim().to_string();
    if let Err(message) = check_pull_id(&model) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response();
    }
    let abort = Arc::new(AtomicBool::new(false));
    if !payload.stream {
        return match pull(&model, |_| {}, abort).await {
            Ok(result) => Json(result).into_response(),
            Err(error) => (error.status(), Json(json!({ "error": error.to_string() }))).into_response(),
        };
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let lines = tx.clone();
    let stop = abort.clone();
    tokio::spawn(async move {
        let last = match pull(&model, move |update| {
            if lines.send(update).is_err() {
                // The client went away
                stop.store(true, Ordering::Relaxed);
            }
        }, abort)
        .await
        {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }),
        };
        let _ = tx.send(last);
    });
    let stream = async_stream::stream! {
        while let Some(line) = rx.recv().await {
            yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(format!("{line}\n")));
        }
    };
    ([(axum::http::header::CONTENT_TYPE, "application/x-ndjson")], Body::from_stream(stream)).into_response()
}

/// A model id, Hugging Face repo or URL as `cluaiz pull` takes it
pub(crate) fn check_pull_id(model: &str) -> Result<(), &'static str> {
    if model.is_empty() || model.len() > 512 || model.chars().any(char::is_control) {
        return Err("`model` must be a model id, a Hugging Face repo (owner/name[:QUANT]) or its URL");
    }
    Ok(())
}

/// Why a pull did not finish
pub(crate) enum PullError {
    /// The same model is being pulled already
    Busy,
    Failed(String),
}

impl PullError {
    fn status(&self) -> StatusCode {
        match self {
            Self::Busy => StatusCode::CONFLICT,
            Self::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(f, "This model is being pulled already"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

/// Models being pulled now: a second pull of the same model is refused instead of racing it
static PULLING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Resolves `model` (the `:QUANT` variant, else the first one), downloads and registers it: what
/// `cluaiz pull` does, without questions. Used by `/api/pull` and CEP `models.download`.
/// `on_update` gets Ollama-shaped lines: `{"status"}` per step, plus `total` / `completed` (bytes)
/// at most once per percent. The answer is `{"status":"success", "model", "path"?, "downloaded"}`.
pub(crate) async fn pull(
    model: &str,
    on_update: impl Fn(Value) + Send + Sync + 'static,
    abort: Arc<AtomicBool>,
) -> Result<Value, PullError> {
    let key = model.to_lowercase();
    if !PULLING.lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone()) {
        return Err(PullError::Busy);
    }
    struct Release(String);
    impl Drop for Release {
        fn drop(&mut self) {
            PULLING.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
        }
    }
    let _release = Release(key);

    let on_update = Arc::new(on_update);
    let (tx, mut rx) = mpsc::channel(100);
    let bytes = on_update.clone();
    let forward = tokio::spawn(async move {
        let mut last_percent = None;
        while let Some(event) = rx.recv().await {
            if let DownloadEvent::Progress(percent, completed, total, _, _) = event {
                let percent = percent as u32;
                if last_percent != Some(percent) {
                    last_percent = Some(percent);
                    bytes(json!({ "status": "downloading", "total": total, "completed": completed }));
                }
            }
        }
    });
    let steps = on_update.clone();
    let pulled = cluaiz_models::fetcher::pull(model, move |step| steps(json!({ "status": step.to_string() })), tx, abort).await;
    let _ = forward.await;
    let (model, path, downloaded) = match pulled.map_err(PullError::Failed)? {
        Pulled::Downloaded(path) => (InstalledStateRegistry::id_for_path(&path), Some(path), true),
        Pulled::OnDisk(path) => (InstalledStateRegistry::id_for_path(&path), Some(path), false),
        Pulled::AlreadyInstalled(id) => (id, None, false),
    };
    Ok(json!({ "status": "success", "model": model, "path": path, "downloaded": downloaded }))
}
