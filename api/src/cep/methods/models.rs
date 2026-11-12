//! `models.*`: installed models, load, inspect, remove, and download (a task).

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
struct RoomCandidate {
    /// The model's id (an installed one, or one of a catalog)
    id: String,
    /// Its weight file's size in bytes
    bytes: u64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RoomParams {
    /// Embedding models to place, in the caller's order of preference; without it, the installed
    /// embedding models
    #[serde(default)]
    candidates: Option<Vec<RoomCandidate>>,
    /// The installed chat model the embedding model has to run beside; without it, the chat model
    /// loaded now (if any)
    #[serde(default)]
    chat_model: Option<String>,
}

/// Memory an embedding model of `bytes` takes besides its file: llama.cpp's context and compute
/// buffers for a short batch (notes are a sentence or two)
const EMBEDDING_BUFFERS_GB: f64 = 0.15;

/// Where each candidate embedding model would run beside the chat model, and the one to recommend
async fn embedding_room(ctx: &CallCtx, params: RoomParams) -> Result<Value, CepError> {
    let settings = cluaiz_core::Settings::current();
    let hw = cluaiz_hw::HardwareProber::with_live_memory();
    let registry = InstalledStateRegistry::load();
    let loaded = ctx.state.host.active_model_path.lock().await.clone();
    let loaded_path = loaded.as_deref().and_then(|p| Path::new(p).canonicalize().ok());
    let chat = match params.chat_model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(id) => Some(registry.installed_models.get(id).cloned().ok_or_else(|| CepError::not_found("model_not_found", format!("No installed model {id}")))?),
        None => registry.installed_models.values().find(|e| e.primary_path().and_then(|p| p.canonicalize().ok()).is_some_and(|p| Some(p) == loaded_path)).cloned(),
    };
    // A chat model that is not loaded yet still needs its room: its plan on today's free memory
    let mut chat_report = Value::Null;
    // Taken by the chat model when it loads (nothing when it is loaded: free memory shows it), and
    // the VRAM its plan keeps free
    let (mut chat_vram, mut chat_ram, mut kept_free) = (0.0, 0.0, 0.0);
    if let Some(entry) = &chat {
        let path = entry.primary_path();
        let is_loaded = path.as_ref().and_then(|p| p.canonicalize().ok()).is_some_and(|p| Some(p) == loaded_path);
        let plan = if is_loaded {
            ctx.state.host.current_plan.lock().await.clone()
        } else {
            path.filter(|_| entry.format_type.eq_ignore_ascii_case("gguf"))
                .and_then(|p| cluaiz_models::GgufProber::probe_geometry(&p).ok())
                .map(|geometry| cluaiz_plan::Planner::create_plan(&entry.id, &geometry, &hw, &settings))
        };
        if let (Some(plan), false) = (&plan, is_loaded) {
            (chat_vram, chat_ram, kept_free) = (plan.vram_budget_gb, plan.ram_budget_gb, plan.vram_buffer_gb.max(0.0));
        }
        chat_report = json!({ "id": entry.id, "loaded": is_loaded, "placement": plan.as_ref().map(|p| p.placement_label()),
                              "vram_gb": plan.as_ref().map(|p| p.vram_budget_gb), "ram_gb": plan.as_ref().map(|p| p.ram_budget_gb) });
    }
    let gpu_free = hw.gpus.first().map_or(0.0, |g| g.vram_available_gb);
    let gpu_left = (gpu_free - chat_vram - kept_free.max(cluaiz_plan::margin::reserve_gb(&hw))).max(0.0);
    let ram_left = (cluaiz_core::bytes_to_gib(cluaiz_plan::ram_budget_bytes(&hw, &settings)) - chat_ram).max(0.0);
    let candidates: Vec<(String, u64, bool)> = match params.candidates {
        Some(list) => list.into_iter().map(|c| {
            let installed = registry.installed_models.contains_key(&c.id);
            (c.id, c.bytes, installed)
        }).collect(),
        None => {
            let mut found: Vec<(String, u64, bool)> = registry
                .installed_models
                .values()
                .filter(|e| crate::handlers::embeddings::is_embedding_model(e))
                .filter_map(|e| Some((e.id.clone(), std::fs::metadata(e.primary_path()?).ok()?.len(), true)))
                .collect();
            found.sort();
            found
        }
    };
    let mut recommended: Option<usize> = None;
    let mut rows = Vec::new();
    for (i, (id, bytes, installed)) in candidates.iter().enumerate() {
        let need = cluaiz_core::bytes_to_gib(*bytes) + EMBEDDING_BUFFERS_GB;
        let runs_on = if !hw.gpus.is_empty() && need <= gpu_left { "gpu" } else if need <= ram_left { "cpu" } else { "none" };
        rows.push(json!({ "id": id, "bytes": bytes, "installed": installed, "needs_gb": (need * 100.0).round() / 100.0, "runs_on": runs_on }));
        let better = match (recommended.map(|r| rows[r]["runs_on"].as_str() == Some("gpu")), runs_on) {
            (None, "gpu" | "cpu") => true,
            (Some(false), "gpu") => true,
            _ => false,
        };
        if better {
            recommended = Some(i);
        }
    }
    if let Some(i) = recommended {
        rows[i]["recommended"] = json!(true);
    }
    Ok(json!({
        "chat_model": chat_report,
        "gpu_left_gb": (gpu_left * 100.0).round() / 100.0,
        "ram_left_gb": (ram_left * 100.0).round() / 100.0,
        "candidates": rows,
    }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListParams {
    /// Only models of this category; all when absent
    category: Option<Category>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModelParams {
    /// An installed model id (from `models.list`)
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
    /// An installed model id (from `models.list`)
    model: String,
    /// A file inside the model's folder, relative to it (default: the primary weight file)
    file: Option<String>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "models.list",
        Meta::new(
            Scope::Read,
            "Installed registry entries, optionally filtered by category",
        )
        .example(json!({"category": "stt"}), json!([{
            "id": "whisper-large-v3-turbo-INT8", "category": "stt", "format": "onnx", "size": 1950984246, "experimental": false,
            "huggingface_repo": "onnx-community/whisper-large-v3-turbo",
            "local_dir": "C:/Users/me/.cluaiz/models/stt/whisper-large-v3-turbo-INT8",
            "files": [{"name": "decoder_model_int8.onnx", "size_bytes": 439684851, "is_primary": true},
                      {"name": "encoder_model_int8.onnx", "size_bytes": 644822094, "is_primary": false}],
            "extra_files": ["config.json", "tokenizer.json"],
            "supported_tasks": ["speech_to_text", "automatic-speech-recognition"],
            "metadata": {"architecture": "onnx-generic", "backend_type": "onnx"}
        }])),
        |_: CallCtx, _: Call, params: ListParams| async move { worker(move || list(params)).await },
    );
    registry.add(
        "models.embedding_room",
        Meta::new(
            Scope::Read,
            "Where each embedding model would run beside the chat model (GPU, CPU or no room) on today's free memory, and the one to recommend",
        )
        .error(-32004, "model_not_found", "`chat_model` is not an installed model")
        .example(
            json!({"candidates":[{"id":"embeddinggemma-300m-gguf-Q8_0","bytes":333590944},{"id":"qwen3-embedding-0.6b-gguf-Q8_0","bytes":639150592}]}),
            json!({"chat_model":{"id":"qwen3.8-4b-distill-gguf-Q4_K_M","loaded":true,"placement":"GPU + RAM split","vram_gb":2.44,"ram_gb":1.6},
                   "gpu_left_gb":0.12,"ram_left_gb":9.8,
                   "candidates":[{"id":"embeddinggemma-300m-gguf-Q8_0","bytes":333590944,"installed":true,"needs_gb":0.46,"runs_on":"cpu","recommended":true},
                                 {"id":"qwen3-embedding-0.6b-gguf-Q8_0","bytes":639150592,"installed":false,"needs_gb":0.75,"runs_on":"cpu"}]}),
        ),
        |ctx: CallCtx, _: Call, params: RoomParams| async move { embedding_room(&ctx, params).await },
    );
    registry.add(
        "models.inspect",
        Meta::new(
            Scope::Read,
            "Raw GGUF metadata and tensor table of an installed model",
        )
        .error(-32004, "model_not_found", "No installed model with this id, or its weight file is missing")
        .error(-32005, "provider_model", "The model is a provider's (`<provider>/<model>` in providers.json); this method works on installed models only")
        .error(-32602, "invalid_params", "`model` is empty or looks like a path")
        .error(-32002, "forbidden", "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32602, "invalid_params", "`file` leaves the model's folder")
        .error(-32010, "gguf_probe_failed", "The file is not a readable GGUF")
        .example(
            json!({"model":"bonsai-27b-gguf-q1_0"}),
            json!({"model":"bonsai-27b-gguf-q1_0","file":"C:/Users/me/.cluaiz/models/chat/bonsai-27b-gguf-q1_0/Bonsai-27B-Q1_0.gguf",
                   "metadata":{"general.architecture":"qwen35","general.name":"Bonsai 27B","qwen35.context_length":262144,"qwen35.block_count":64},
                   "int_arrays":{"qwen35.rope.dimension_sections":[11,11,10,0]},"speech_languages":[],
                   "tensors":[{"name":"output.weight","dims":[5120,248320],"ggml_type":41,"offset":0,"size_bytes":178790400}]}),
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
        .error(-32004, "model_not_found", "No installed model with this id, or its weight file is missing")
        .error(-32005, "provider_model", "The model is a provider's (`<provider>/<model>` in providers.json); this method works on installed models only")
        .error(-32602, "invalid_params", "`model` is empty or looks like a path")
        .error(-32002, "forbidden", "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32007, "not_implemented", "The model is not a GGUF (ONNX models load on their first request)")
        .error(-32010, "model_load_failed", "llama.cpp could not load it (memory, a broken file, an unsupported architecture)")
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
        .error(-32004, "model_not_found", "No installed model with this id, or its weight file is missing")
        .error(-32005, "provider_model", "The model is a provider's (`<provider>/<model>` in providers.json); this method works on installed models only")
        .error(-32602, "invalid_params", "`model` is empty or looks like a path")
        .error(-32002, "forbidden", "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "model_remove_failed", "The folder or the registry entry could not be removed")
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
                cluaiz_runtime::vram_use::forget_model(&entry.id);
                crate::handlers::chat::semantic::forget_model(&entry.id);
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
        .error(-32602, "invalid_params", "`model` is empty, over 512 characters, or has control characters")
        .error(-32005, "provider_model", "The model is a provider's (`<provider>/<model>` in providers.json); this method works on installed models only")
        .error(-32005, "pull_running", "This model is being downloaded already (from the task's result)")
        .error(-32010, "download_failed", "Resolving or downloading failed: no such repo or variant, network, disk (from the task's result)")
        .error(-32008, "cancelled", "`tasks.cancel` stopped the download (from the task's result)")
        .example(json!({"model": "owner/name-GGUF:Q4_K_M"}), json!({"task": {"task_id": "t_5f0c…", "method": "models.download", "status": "working"}})),
        |_: CallCtx, call: Call, params: DownloadParams| async move {
            let model = params.model.trim().to_string();
            models::check_pull_id(&model).map_err(|message| roots::invalid("model", message))?;
            crate::providers::refuse_provider_model(&model, "models.download")?;
            call.start_task(move |task| async move {
                let abort = task.cancel_flag();
                let updates = task.clone();
                // Bytes become the task's progress; every other step is an event
                let on_update = move |update: Value| match (update["completed"].as_u64(), update["total"].as_u64()) {
                    (Some(done), total) => updates.progress(Progress { done, total, unit: Some("bytes".into()), message: None }),
                    _ => updates.event(update),
                };
                let pulled = models::pull(&model, on_update, abort).await.map_err(|error| match error {
                    models::PullError::Busy => CepError::conflict("pull_running", error.to_string()),
                    models::PullError::Failed(message) => CepError::unavailable("download_failed", message),
                })?;
                cluaiz_cep::events::publish("model.downloaded", json!({ "model": model }));
                Ok(pulled)
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
    crate::providers::refuse_provider_model(&model, "this method")?;
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
