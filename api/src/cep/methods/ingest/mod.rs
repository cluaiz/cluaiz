//! `ingest.run`: documents and media to text, chunks or vectors, as a task. Each source's result
//! is a task event as it finishes; the task's result holds all of them. Source access goes through
//! the same canonicalizer as chat attachments, then the caller's roots.

mod model;

use crate::cep::{error_from_response, CallCtx};
use crate::utils::authorize_source_for_call;
use cluaiz_cep::{Call, CepError, Meta, Progress, Registry, Scope, TaskHandle};
use cluaiz_media::{
    documents::{self, DEFAULT_CHUNK_BYTES},
    source::SourceResolver,
};
use model::{document_item, ModelFailure, Processor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputControls {
    /// Include the extracted text (default true)
    pub return_text: Option<bool>,
    /// Vectors from the selected embedding model
    pub return_embeddings: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkingStrategy {
    /// Only "semantic"
    pub r#type: Option<String>,
    /// Maximum UTF-8 bytes in a returned chunk, not model tokens
    pub max_chunk_size: Option<usize>,
    /// Only 0
    pub overlap: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisionSettings {
    /// false: a vision model's text is not generated (only with a vision `model`)
    pub use_vision: Option<bool>,
    /// Not supported: any value is refused
    pub detail_level: Option<String>,
    /// The instruction the vision model gets with each page or image (only with a vision `model`)
    pub system_instruction: Option<String>,
    /// Most tokens the vision model writes per page or image (only with a vision `model`)
    pub max_tokens: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum SourceInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestPayload {
    /// A path (inside the caller's roots), an http(s) URL or a data URI; or a list of them
    pub source: SourceInput,
    /// An installed embedding, vision or chat model; extraction only when absent
    pub model: Option<String>,
    /// What each source's result contains
    pub output_controls: Option<OutputControls>,
    /// How extracted text is cut into chunks
    pub chunking_strategy: Option<ChunkingStrategy>,
    /// Options for a generative vision `model`
    pub vision_settings: Option<VisionSettings>,
}

fn invalid(message: &str) -> CepError {
    CepError::invalid_params(message, Vec::new(), None)
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "ingest.run",
        Meta::new(Scope::Read, "Extracts documents (and optionally runs a model on them), as a task; one event per source")
            .task()
            .error(-32602, "invalid_params", "No source, an empty source, unsupported chunking (overlap or a type other than semantic), vision settings without a vision model, or vectors without an embedding model")
            .error(-32602, "invalid_params", "A source path does not exist or is drive-relative / root-relative")
            .error(-32004, "model_not_found", "`model` is not installed")
            .error(-32002, "forbidden", "A source path is outside the caller's roots (`data.needs: \"roots\"`)")
            .error(-32004, "ingest_failed", "Every source failed and the first failure was a missing file or model (from the task's result; `data.result` holds every source)")
            .error(-32602, "ingest_failed", "Every source failed and the first failure was a bad input (from the task's result)")
            .error(-32005, "ingest_failed", "Every source failed and the first failure was a busy model (from the task's result)")
            .error(-32006, "ingest_failed", "Every source failed and the first failure was a source over the size limit (from the task's result)")
            .error(-32010, "ingest_failed", "Every source failed and the first failure was the model or the machine (from the task's result)")
            .example(
                json!({ "source": "docs/report.pdf", "chunking_strategy": { "max_chunk_size": 2000 } }),
                json!({ "task": { "task_id": "t_5f0c…", "method": "ingest.run", "status": "working" } }),
            ),
        run,
    );
}

/// Everything checked before the task starts, so a bad request fails at once
async fn run(ctx: CallCtx, call: Call, payload: IngestPayload) -> Result<Value, CepError> {
    let processor = match Processor::select(&payload) {
        Ok(processor) => processor,
        Err(response) => return Err(error_from_response(response).await),
    };
    let sources = match payload.source {
        SourceInput::Single(source) => vec![source],
        SourceInput::Multiple(sources) => sources,
    };
    if sources.is_empty() || sources.iter().any(|source| source.trim().is_empty()) {
        return Err(invalid("At least one non-empty document source is required"));
    }
    let strategy = payload.chunking_strategy.as_ref();
    if strategy.is_some_and(|s| s.overlap.unwrap_or(0) != 0 || s.r#type.as_deref().is_some_and(|t| t != "semantic")) {
        return Err(invalid("Only semantic chunking with zero overlap is currently supported"));
    }
    let chunk_size = strategy.and_then(|s| s.max_chunk_size).unwrap_or(DEFAULT_CHUNK_BYTES);
    let settings = cluaiz_core::Settings::current();
    if chunk_size == 0 || u64::try_from(chunk_size).map_or(true, |size| size > settings.media.limits.max_decoded_bytes()) {
        return Err(invalid("Chunk size must be positive and fit the document byte limit"));
    }
    let return_text = payload.output_controls.as_ref().and_then(|c| c.return_text).unwrap_or(true);
    // Every source is authorized before any work, so an outside path asks (or fails) at once
    let mut authorized = Vec::with_capacity(sources.len());
    for original in sources {
        let mut source = original.clone();
        authorize_source_for_call(&ctx.state, &call, &mut source).await?;
        authorized.push((original, source));
    }
    let job = Job { processor, chunk_size, return_text, settings };
    call.start_task(move |task| async move { job.run(ctx, authorized, task).await })
}

struct Job {
    processor: Option<Processor>,
    chunk_size: usize,
    return_text: bool,
    settings: cluaiz_core::ActiveUserSettings,
}

impl Job {
    async fn run(self, ctx: CallCtx, sources: Vec<(String, String)>, task: TaskHandle) -> Result<Value, CepError> {
        let settings = &self.settings;
        let resolver = SourceResolver::from_environment(settings.media).map_err(|e| CepError::unavailable("source_error", e.to_string()))?;
        let total = sources.len() as u64;
        let mut processed = Vec::new();
        let (mut input_bytes, mut output_bytes) = (0u64, 0u64);
        for (done, (original, source)) in sources.into_iter().enumerate() {
            if task.is_cancelled() {
                return Err(CepError::cancelled());
            }
            let result = async {
                // The source was canonicalized and authorized before the task started
                let resolved = resolver.resolve(&source, |_| Ok(())).await?;
                input_bytes = input_bytes.checked_add(resolved.input_bytes).ok_or_else(|| anyhow::anyhow!("Document batch size overflow"))?;
                settings.media.limits.check_input_bytes(input_bytes)?;
                let bytes = resolved.read_bytes(settings.media.limits).await?;
                let format = resolved.format.clone();
                drop(resolved);
                let mut item = if let Some(processor) = &self.processor {
                    processor.process(&ctx.state, bytes, format, settings.clone(), self.chunk_size, self.return_text).await?
                } else {
                    let limits = settings.media.limits;
                    let document = tokio::task::spawn_blocking(move || documents::extract(&bytes, &format, limits)).await??;
                    document_item(document, limits, self.chunk_size, self.return_text)?
                };
                item["source"] = json!(original);
                Ok::<_, anyhow::Error>(item)
            }
            .await;
            let item = result.unwrap_or_else(|error| {
                let mut item = json!({ "source": original, "status": "error", "error": format!("{error:#}") });
                if let Some(failure) = error.downcast_ref::<ModelFailure>() {
                    item["http_status"] = json!(failure.status.as_u16());
                    item["model_error"] = failure.body.clone();
                }
                item
            });
            let size = serde_json::to_vec(&item).map_err(|_| CepError::internal("ingest.run"))?.len() as u64;
            output_bytes = output_bytes.checked_add(size).ok_or_else(|| invalid("Document output size overflow"))?;
            if settings.media.limits.check_decoded_bytes(output_bytes).is_err() {
                return Err(CepError::too_large(settings.media.limits.max_decoded_bytes(), output_bytes));
            }
            task.event(item.clone());
            task.progress(Progress { done: done as u64 + 1, total: Some(total), unit: Some("sources".into()), message: None });
            processed.push(item);
        }
        let ingested = processed.iter().filter(|item| item["status"] == "ingested").count();
        let status = if ingested == processed.len() {
            "success"
        } else if processed.iter().all(|item| item["status"] == "error") {
            "error"
        } else {
            "partial"
        };
        let result = json!({ "status": status, "total_sources": processed.len(), "processed_sources": processed });
        if status == "error" {
            // Nothing came out: the task fails, with every source's reason
            let first = processed.first();
            let message = first.and_then(|item| item["error"].as_str()).unwrap_or("No source was ingested").to_string();
            // A model's own failure keeps its meaning (missing model, busy backend…)
            let code = match first.and_then(|item| item["http_status"].as_u64()) {
                Some(404) => cluaiz_cep::error::NOT_FOUND,
                Some(409) => cluaiz_cep::error::CONFLICT,
                Some(413) => cluaiz_cep::error::TOO_LARGE,
                Some(500..=599) => cluaiz_cep::error::UNAVAILABLE,
                _ => cluaiz_cep::error::INVALID_PARAMS,
            };
            return Err(CepError::new(code, message, Some(json!({ "type": "ingest_failed", "result": result }))));
        }
        Ok(result)
    }
}
