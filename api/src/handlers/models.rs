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
    // The models listed in providers.json, by their full name (`<provider>/<model>`)
    for (id, context, kind) in cluaiz_core::providers::Providers::load_or_empty().listed_models() {
        let provider = id.split('/').next().unwrap_or_default().to_string();
        data.push(json!({
            "id": id,
            "object": "model",
            "created": now,
            "owned_by": provider,
            "permission": [],
            "root": id,
            "parent": null,
            "category": kind,
            "context_window": if kind == "chat" { json!(context) } else { Value::Null },
            "provider": provider,
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
            // The main file's change time, as Ollama sends it; null when the file cannot be read
            let modified_at = entry
                .primary_path()
                .and_then(|path| std::fs::metadata(path).ok())
                .and_then(|meta| meta.modified().ok())
                .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            json!({
                "name": entry.id,
                "model": entry.id,
                "modified_at": modified_at,
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

/// Ollama's `/api/show` body (`api/types.go`, `ShowRequest`): `model`, or the older `name`.
/// Its other fields (`verbose`, `system`, `template`, `options`) are accepted and not used.
#[derive(Debug, Deserialize)]
pub struct ShowPayload {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub name: String,
}

fn ollama_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// POST /api/show (Ollama): an installed model's GGUF header (the data CEP `models.inspect`
/// reads) in Ollama's `ShowResponse` shape (`server/routes.go`, `GetModelInfo`). Nothing is loaded.
pub async fn show(body: axum::body::Bytes) -> Response {
    if body.iter().all(u8::is_ascii_whitespace) {
        return ollama_error(StatusCode::BAD_REQUEST, "missing request body".into());
    }
    let payload: ShowPayload = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => return ollama_error(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let model = if payload.model.trim().is_empty() { payload.name } else { payload.model }.trim().to_string();
    if model.is_empty() {
        return ollama_error(StatusCode::BAD_REQUEST, "model is required".into());
    }
    match tokio::task::spawn_blocking(move || show_model(&model)).await {
        Ok(result) => result.unwrap_or_else(|(status, message)| ollama_error(status, message)),
        Err(error) => ollama_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn show_model(model: &str) -> Result<Response, (StatusCode, String)> {
    let not_found = || (StatusCode::NOT_FOUND, format!("model '{model}' not found"));
    let entry = InstalledStateRegistry::find(model).ok_or_else(not_found)?;
    let primary = entry.primary_path().ok_or_else(not_found)?;
    let modified_at = std::fs::metadata(&primary)
        .and_then(|meta| meta.modified())
        .ok()
        .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let file_name = primary.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let mut details = json!({
        "parent_model": "",
        "format": entry.format_type,
        "family": entry.metadata.architecture,
        "families": [entry.metadata.architecture],
        "parameter_size": entry.metadata.parameters,
        "quantization_level": cluaiz_models::taxonomy::UniversalQuantization::extract_quant_tag(&file_name),
    });
    // Ollama always sends `model_info` (an empty object when it has nothing), as clients expect it
    let mut model_info = serde_json::Map::new();
    let mut template = entry.metadata.chat_template.clone().unwrap_or_default();
    if entry.format_type == "gguf" {
        let header = cluaiz_models::GgufProber::read_header(&primary)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
        if let Some(chat_template) = header.metadata.get("tokenizer.chat_template") {
            template = chat_template.clone();
        }
        for (key, value) in &header.metadata {
            // Ollama leaves these two out of model_info (the template has its own field)
            if key == "general.name" || key == "tokenizer.chat_template" {
                continue;
            }
            let value = match header.int_arrays.get(key) {
                Some(values) => json!(values),
                None => gguf_value(value),
            };
            model_info.insert(key.clone(), value);
        }
        let parameters = gguf_parameter_count(&entry, &primary, &header);
        if parameters > 0 {
            model_info.insert("general.parameter_count".into(), json!(parameters));
            details["parameter_size"] = json!(human_number(parameters));
        }
    }
    Ok(Json(json!({
        "modelfile": "",
        "parameters": "",
        "template": template,
        "details": details,
        "model_info": model_info,
        "modified_at": modified_at,
    }))
    .into_response())
}

/// A header value as Ollama types it: the Engine's GGUF reader keeps every scalar as text and an
/// array as a "[...Array: len=N ...]" summary. Numbers and booleans go back to JSON numbers and
/// booleans; a skipped array is `[]`, as Ollama sends an array longer than its read limit
/// (`fs/gguf/metadata.go`, `Values`: `skippedArray` becomes `[]any{}`).
fn gguf_value(text: &str) -> Value {
    if text.starts_with('[') && text.contains("Array: len=") {
        return json!([]);
    }
    if let Ok(number) = text.parse::<i64>() {
        return json!(number);
    }
    if let Ok(number) = text.parse::<f64>() {
        if number.is_finite() {
            return json!(number);
        }
    }
    match text {
        "true" => json!(true),
        "false" => json!(false),
        _ => json!(text),
    }
}

/// Weights of an installed GGUF model counted from its tensor table (no model load). A split model
/// adds the other parts listed in the registry (same `gguf-split` prefix and count); 0 when unknown.
fn gguf_parameter_count(
    entry: &cluaiz_models::types::manifest::ModelRegistryEntry,
    primary: &std::path::Path,
    header: &cluaiz_models::prober::gguf::GgufHeader,
) -> u64 {
    use cluaiz_models::taxonomy::UniversalQuantization;
    let primary_name = primary.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let Some((prefix, _, count)) = UniversalQuantization::gguf_split(&primary_name).filter(|(_, _, count)| *count > 1) else {
        return header.parameter_count();
    };
    let dir = std::path::Path::new(&entry.local_dir);
    let parts: Vec<_> = entry
        .files
        .iter()
        .filter(|file| UniversalQuantization::gguf_split(&file.name).is_some_and(|(p, _, c)| p == prefix && c == count))
        .map(|file| dir.join(&file.name))
        .collect();
    if parts.len() != count as usize {
        return 0;
    }
    let mut total = 0;
    for part in parts {
        match cluaiz_models::GgufProber::read_header(&part) {
            Ok(part_header) => total += part_header.parameter_count(),
            Err(_) => return 0,
        }
    }
    total
}

/// Ollama's parameter size text (`format/format.go`, `HumanNumber`): 6.9B, 125.08M, 512K
fn human_number(count: u64) -> String {
    const THOUSAND: f64 = 1e3;
    const MILLION: f64 = 1e6;
    const BILLION: f64 = 1e9;
    let value = count as f64;
    if value >= BILLION {
        let number = value / BILLION;
        if number == number.floor() { format!("{number:.0}B") } else { format!("{number:.1}B") }
    } else if value >= MILLION {
        let number = value / MILLION;
        if number == number.floor() { format!("{number:.0}M") } else { format!("{number:.2}M") }
    } else if value >= THOUSAND {
        format!("{:.0}K", value / THOUSAND)
    } else {
        count.to_string()
    }
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

/// Removes what a stopped download of `model` left (its `.part` and finished files). A download that is
/// still stopping is waited for (a few seconds); one still running is refused. Answers
/// `{"model", "removed"}`: the folder removed, or null when nothing was left.
pub(crate) async fn discard(model: &str) -> Result<Value, PullError> {
    let key = model.to_lowercase();
    for _ in 0..20 {
        if !PULLING.lock().unwrap_or_else(|e| e.into_inner()).contains(&key) {
            let removed = cluaiz_models::fetcher::discard(model).await.map_err(PullError::Failed)?;
            return Ok(json!({ "model": model, "removed": removed }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(PullError::Busy)
}

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
