//! `models.*: installed models, load, inspect, remove, download` (03-method-catalog Part B). Owner: Wave B (05-chatgpt-plan.md §2).

use crate::cep::{roots, CallCtx};
use crate::handlers::models;
use cluaiz_cep::{Call, CepError, Meta, Progress, Registry, Scope};
use cluaiz_models::{registry::InstalledStateRegistry, types::manifest::ModelRegistryEntry};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum Category {
    Chat,
    Embedding,
    VisionEmbedding,
    Vision,
    Tts,
    Stt,
}

impl Category {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::VisionEmbedding => "vision-embedding",
            Self::Vision => "vision",
            Self::Tts => "tts",
            Self::Stt => "stt",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListParams {
    category: Option<Category>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModelParams {
    model: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DownloadParams {
    /// A catalog id, or a Hugging Face repo `owner/name` (optionally `:QUANT`, an `hf://` or https URL)
    model: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectParams {
    model: String,
    file: Option<String>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "models.list",
        Meta::new(
            Scope::Read,
            "Installed registry entries, optionally filtered by category",
        )
        .example(json!({}), json!([])),
        |_: CallCtx, _: Call, params: ListParams| async move { worker(move || list(params)).await },
    );
    registry.add(
        "models.inspect",
        Meta::new(
            Scope::Read,
            "Raw GGUF metadata and tensor table of an installed model",
        )
        .example(
            json!({"model":"installed-model"}),
            json!({"model":"installed-model","metadata":{},"tensors":[]}),
        ),
        |ctx: CallCtx, call: Call, params: InspectParams| async move {
            let (entry, path) = selected(&ctx, call, params.model, params.file, false).await?;
            worker(move || inspect(&entry.id, &path)).await
        },
    );
    registry.add(
        "models.load",
        Meta::new(
            Scope::Write,
            "Loads an installed GGUF model using the existing Engine host",
        )
        .example(
            json!({"model":"installed-model"}),
            json!({"model":"installed-model","loaded_ms":125}),
        ),
        |ctx: CallCtx, call: Call, params: ModelParams| async move {
            let (entry, path) = selected(&ctx, call, params.model, None, false).await?;
            if entry.format_type != "gguf" {
                return Err(CepError::not_implemented(
                    "Explicit models.load for this model format",
                ));
            }
            let started = std::time::Instant::now();
            ctx.state
                .host
                .ensure_loaded(&entry.id, &path, &cluaiz_core::Settings::current())
                .await
                .map_err(|error| CepError::unavailable("model_load_failed", error.to_string()))?;
            Ok(json!({"model":entry.id,"loaded_ms":started.elapsed().as_millis()}))
        },
    );
    registry.add(
        "models.remove",
        Meta::new(
            Scope::Write,
            "Unloads the selected model if active and removes its installed package",
        )
        .destructive()
        .example(
            json!({"model":"installed-model"}),
            json!({"model":"installed-model","dir":"models/chat/installed-model"}),
        ),
        |ctx: CallCtx, call: Call, params: ModelParams| async move {
            let (entry, directory) = selected(&ctx, call, params.model, None, true).await?;
            let loaded = ctx.state.host.active_model_path.lock().await.clone();
            if loaded
                .and_then(|path| Path::new(&path).canonicalize().ok())
                .is_some_and(|path| path.starts_with(&directory))
            {
                ctx.state.host.unload_model().await;
            }
            worker(move || {
                let directory = cluaiz_models::ModelDownloader::uninstall(&entry.id)
                    .map_err(|error| CepError::unavailable("model_remove_failed", error))?;
                Ok(json!({"model":entry.id,"dir":roots::display(&directory)}))
            })
            .await
        },
    );
    registry.add(
        "models.download",
        Meta::new(
            Scope::Write,
            "Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first",
        )
        .task()
        .idempotent(false)
        .example(json!({"model": "owner/name-GGUF:Q4_K_M"}), json!({"task": {"task_id": "t_5f0c…", "method": "models.download", "status": "working"}})),
        |_: CallCtx, call: Call, params: DownloadParams| async move {
            let model = params.model.trim().to_string();
            models::check_pull_id(&model).map_err(|message| roots::invalid("model", message))?;
            call.start_task(move |task| async move {
                let abort = task.cancel_flag();
                let updates = task.clone();
                // Bytes become the task's progress; every other step is an event
                let on_update = move |update: Value| match (update["completed"].as_u64(), update["total"].as_u64()) {
                    (Some(done), total) => updates.progress(Progress { done, total, unit: Some("bytes".into()), message: None }),
                    _ => updates.event(update),
                };
                models::pull(&model, on_update, abort).await.map_err(|error| match error {
                    models::PullError::Busy => CepError::conflict("pull_running", error.to_string()),
                    models::PullError::Failed(message) => CepError::unavailable("download_failed", message),
                })
            })
        },
    );
}

fn validate_id(model: &str) -> Result<(), CepError> {
    if model.trim().is_empty() || model.contains("..") || model.contains(['/', '\\', '\0']) {
        return Err(roots::invalid(
            "model",
            "Use an installed model ID, not a filesystem path",
        ));
    }
    Ok(())
}

async fn worker<T, F>(work: F) -> Result<T, CepError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CepError> + Send + 'static,
{
    tokio::task::spawn_blocking(work).await.map_err(|error| {
        let trace = uuid::Uuid::new_v4().to_string();
        tracing::error!(trace_id = %trace, %error, "CEP model worker failed");
        CepError::internal(&trace)
    })?
}

fn list(params: ListParams) -> Result<Value, CepError> {
    // The same domain registry load/refresh used by /v1/models and /api/tags.
    let registry = InstalledStateRegistry::load();
    let mut entries: Vec<_> = registry
        .installed_models
        .into_values()
        .filter(|entry| {
            params
                .category
                .as_ref()
                .is_none_or(|category| entry.category == category.as_str())
        })
        .collect();
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    let entries: Result<Vec<_>, _> = entries
        .into_iter()
        .map(|entry| {
            let experimental = entry
                .metadata
                .tts_family
                .as_deref()
                .is_some_and(cluaiz_runtime::onnx_host::is_experimental_family);
            let mut value = serde_json::to_value(&entry).map_err(|error| {
                CepError::unavailable("model_metadata_invalid", error.to_string())
            })?;
            value["format"] = json!(entry.format_type);
            value["size"] = json!(entry.size_bytes());
            value["experimental"] = json!(experimental);
            if let Some(object) = value.as_object_mut() {
                object.remove("format_type");
            }
            Ok::<_, CepError>(value)
        })
        .collect();
    Ok(json!(entries?))
}

async fn selected(
    ctx: &CallCtx,
    call: Call,
    model: String,
    file: Option<String>,
    directory: bool,
) -> Result<(ModelRegistryEntry, PathBuf), CepError> {
    validate_id(&model)?;
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    worker(move || {
        let entry = InstalledStateRegistry::find(&model).ok_or_else(|| {
            CepError::not_found(
                "model_not_found",
                format!("Model '{model}' is not installed"),
            )
        })?;
        let path = if directory {
            PathBuf::from(&entry.local_dir)
        } else if let Some(file) = file {
            let relative = Path::new(&file);
            if file.is_empty()
                || file.contains('\0')
                || relative.is_absolute()
                || relative
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
            {
                return Err(roots::invalid(
                    "file",
                    "file must stay inside the selected model package",
                ));
            }
            let base = roots::resolve(&entry.local_dir, &workspace, false)?;
            let path = roots::resolve(&base.join(relative).to_string_lossy(), &workspace, false)?;
            if !path.starts_with(&base) {
                return Err(roots::invalid(
                    "file",
                    "file leaves the selected model package",
                ));
            }
            path
        } else {
            entry.primary_path().ok_or_else(|| {
                CepError::not_found("model_not_found", "Model weight file is missing")
            })?
        };
        let path = roots::resolve(&path.to_string_lossy(), &workspace, false)?;
        call.check_path(&path)?;
        Ok((entry, path))
    })
    .await
}

fn inspect(model: &str, path: &Path) -> Result<Value, CepError> {
    let header = cluaiz_models::GgufProber::read_header(path)
        .map_err(|error| CepError::unavailable("gguf_probe_failed", error.to_string()))?;
    let tensors: Vec<_> = header
        .tensors
        .iter()
        .map(|tensor| {
            json!({
                "name":tensor.name, "dims":tensor.dims, "ggml_type":tensor.ggml_type,
                "offset":tensor.offset, "size_bytes":tensor.size_bytes
            })
        })
        .collect();
    Ok(
        json!({"model":model,"file":roots::display(path),"metadata":header.metadata,
              "int_arrays":header.int_arrays,"speech_languages":header.speech_languages,
              "tensors":tensors}),
    )
}
