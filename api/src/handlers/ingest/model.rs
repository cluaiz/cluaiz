//! Optional ingest model processing through the existing embedding and native media owners.
use super::{IngestPayload, VisionSettings};
use crate::handlers::embeddings::{self, EmbeddingExecutor};
use crate::{utils::task_error, AppState};
use anyhow::{bail, Context, Result};
use axum::{body::to_bytes, http::StatusCode, response::Response};
use cluaiz_core::{media::PreparedPart, ActiveUserSettings, ChatEnvelope, ChatMessage};
use cluaiz_media::{
    documents::{self, DocumentPart, ExtractedDocument},
    formats::{DocumentKind, MediaFormat},
};
use cluaiz_models::{
    prober::OnnxProber, registry::InstalledStateRegistry, types::manifest::ModelRegistryEntry,
};
use cluaiz_runtime::{media_admission::MediaLease, onnx_host::EmbeddingInput, HostResponse};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(super) struct Processor {
    pub id: String,
    kind: Kind,
}
enum Kind {
    Embedding {
        executor: EmbeddingExecutor,
        image: bool,
    },
    Vision {
        entry: ModelRegistryEntry,
        options: VisionSettings,
    },
}

/// Keep an executor's HTTP status/error when it is nested in an ingest batch.
#[derive(Debug)]
pub(super) struct ModelFailure {
    pub status: StatusCode,
    pub body: Value,
}
impl std::fmt::Display for ModelFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status, self.body)
    }
}
impl std::error::Error for ModelFailure {}

impl Processor {
    pub fn select(payload: &IngestPayload) -> std::result::Result<Option<Self>, Response> {
        let invalid =
            |message: &str| task_error(StatusCode::BAD_REQUEST, "invalid_request_error", message);
        let options = payload.vision_settings.clone().unwrap_or_default();
        if options.detail_level.is_some() {
            return Err(invalid(
                "detail_level is not implemented; use the selected package's processor settings",
            ));
        }
        if options.max_tokens == Some(0) {
            return Err(invalid("vision_settings.max_tokens must be positive"));
        }
        let Some(id) = payload
            .model
            .as_deref()
            .filter(|id| !id.trim().is_empty() && !id.eq_ignore_ascii_case("auto"))
        else {
            if payload
                .output_controls
                .as_ref()
                .is_some_and(|o| o.return_embeddings == Some(true))
            {
                return Err(invalid("Select an embedding model to request vectors"));
            }
            if options.use_vision == Some(true)
                || options.system_instruction.is_some()
                || options.max_tokens.is_some()
            {
                return Err(invalid("Select a vision model to use vision_settings"));
            }
            return Ok(None);
        };
        let registry = InstalledStateRegistry::load();
        let entry = registry
            .installed_models
            .get(id)
            .or_else(|| {
                registry
                    .installed_models
                    .values()
                    .find(|e| e.id.eq_ignore_ascii_case(id))
            })
            .ok_or_else(|| {
                task_error(
                    StatusCode::NOT_FOUND,
                    "model_not_found",
                    "Selected ingest model is not installed",
                )
            })?
            .clone();
        let has_task = |tasks: &[&str]| {
            entry
                .supported_tasks
                .iter()
                .any(|t| tasks.contains(&t.replace('-', "_").to_ascii_lowercase().as_str()))
        };
        let kind = if embeddings::is_embedding_model(&entry) {
            if options.use_vision == Some(true)
                || options.system_instruction.is_some()
                || options.max_tokens.is_some()
            {
                return Err(invalid("Embedding models return vectors; vision text instructions require a generative vision model"));
            }
            if payload
                .output_controls
                .as_ref()
                .is_some_and(|o| o.return_embeddings == Some(false))
            {
                return Err(invalid("The selected model returns embeddings; omit return_embeddings or set it to true"));
            }
            let executor =
                EmbeddingExecutor::new(entry.clone()).map_err(|e| invalid(&format!("{e:#}")))?;
            let image = if entry.format_type.eq_ignore_ascii_case("onnx") {
                let path = entry
                    .primary_path()
                    .ok_or_else(|| invalid("Selected embedding graph is missing"))?;
                OnnxProber::read_signature(&path)
                    .map_err(|e| invalid(&format!("{e:#}")))?
                    .inputs
                    .iter()
                    .any(|i| i.name == "pixel_values")
            } else {
                false
            };
            Kind::Embedding { executor, image }
        } else if entry.format_type.eq_ignore_ascii_case("gguf")
            && has_task(&[
                "multimodal_vision",
                "spatial_vision",
                "document_ocr",
                "vision",
                "vision_chat",
            ])
        {
            if payload
                .output_controls
                .as_ref()
                .is_some_and(|o| o.return_embeddings == Some(true))
            {
                return Err(invalid(
                    "The selected vision model generates text, not embedding vectors",
                ));
            }
            Kind::Vision {
                entry: entry.clone(),
                options,
            }
        } else {
            return Err(invalid("Select a compatible text/image embedding package or generative GGUF vision package; a folder category alone does not establish an OCR decoder"));
        };
        Ok(Some(Self { id: entry.id, kind }))
    }

    pub async fn process(
        &self,
        state: &Arc<AppState>,
        bytes: Vec<u8>,
        format: MediaFormat,
        settings: ActiveUserSettings,
        chunk_size: usize,
        return_text: bool,
    ) -> Result<Value> {
        let source_kind = format.document_kind(&bytes)?;
        let mime_type = format.mime_type.clone();
        let wants_pixels = match &self.kind {
            Kind::Embedding { image, .. } => *image,
            Kind::Vision { options, .. } => options.use_vision != Some(false),
        };
        let pixel_source =
            wants_pixels && matches!(source_kind, DocumentKind::Image | DocumentKind::Pdf);
        if pixel_source {
            if let Kind::Vision { entry, .. } = &self.kind {
                let path = entry
                    .primary_path()
                    .context("Selected vision model is missing")?;
                let package = cluaiz_models::capabilities::PackageCapabilities::inspect(&path)?;
                if package.projector.is_none() {
                    bail!("Selected vision package '{}' has no unambiguous matching media projector; complete the package with its matching mmproj before requesting image/PDF pixels", entry.id);
                }
                if !package
                    .supported_inputs()
                    .contains(&cluaiz_core::Modality::Image)
                {
                    bail!(
                        "Selected vision package '{}' does not declare image input support",
                        entry.id
                    );
                }
            }
        }
        if matches!(&self.kind, Kind::Embedding { image: true, .. }) && !pixel_source {
            bail!("Selected graph requires image pixels; use an image/PDF or choose a text embedding model for DOC/DOCX/text");
        }
        let mut parts = Vec::new();
        let mut vectors = Vec::new();
        let mut output_bytes = 0u64;
        let mut lease = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut guard = cluaiz_media::request::PreparationGuard::new(cancel.clone());
        if matches!(&self.kind, Kind::Vision { .. }) {
            lease = Some(state.host.media_admission.try_acquire().map_err(|_| ModelFailure {
                status: StatusCode::TOO_MANY_REQUESTS, body: json!({"error":{"type":"media_busy","message":"Another attachment request is still active"}})
            })?);
        }
        if pixel_source {
            settings.media.limits.check_decoded_bytes(
                (bytes.len() as u64)
                    .checked_mul(2)
                    .context("Source ownership size overflow")?,
            )?;
            let bytes: Arc<[u8]> = bytes.into();
            let pages = if source_kind == DocumentKind::Pdf {
                let input = bytes.clone();
                let retained = lease.clone();
                tokio::task::spawn_blocking(move || {
                    let _retained = retained;
                    let assets = cluaiz_media::native_assets::NativeAssets::from_environment()?;
                    documents::pdf_render::with_document(&input, &assets, |pdf| pdf.page_count())
                })
                .await??
            } else {
                1
            };
            for page in 1..=pages {
                if cancel.load(Ordering::Acquire) {
                    bail!("Ingest processing cancelled");
                }
                let input = bytes.clone();
                let limits = settings.media.limits;
                let retained = lease.clone();
                let image = tokio::task::spawn_blocking(move || {
                    let _retained = retained;
                    if source_kind == DocumentKind::Pdf {
                        let assets = cluaiz_media::native_assets::NativeAssets::from_environment()?;
                        documents::pdf_render::with_document(&input, &assets, |pdf| {
                            pdf.render_page(page, limits)
                        })
                    } else {
                        cluaiz_media::image::decode_image(&input, limits)
                    }
                })
                .await??;
                let page_number = (source_kind == DocumentKind::Pdf).then_some(page);
                match &self.kind {
                    Kind::Embedding { executor, .. } => {
                        let response = executor
                            .execute(
                                state,
                                vec![EmbeddingInput::Image {
                                    image,
                                    text: String::new(),
                                }],
                                settings.clone(),
                                None,
                            )
                            .await;
                        let mut value = response_json(response, limits.max_decoded_bytes()).await?;
                        value["page"] = json!(page_number);
                        retain_output(&value, &mut output_bytes, &settings)?;
                        vectors.push(value);
                    }
                    Kind::Vision { entry, options } => {
                        let part = vision(
                            state,
                            entry,
                            options,
                            vec![PreparedPart::Image { image }],
                            &settings,
                            lease.clone(),
                            cancel.clone(),
                            page_number,
                        )
                        .await?;
                        retain_output(&serde_json::to_value(&part)?, &mut output_bytes, &settings)?;
                        parts.push(part);
                    }
                }
            }
        } else {
            let limits = settings.media.limits;
            let retained = lease.clone();
            let doc = tokio::task::spawn_blocking(move || {
                let _retained = retained;
                documents::extract(&bytes, &format, limits)
            })
            .await??;
            match &self.kind {
                Kind::Embedding { executor, .. } => {
                    let text = doc.text(limits, false)?;
                    let chunks = documents::chunker::SemanticChunker::chunk(&text, chunk_size)?;
                    let inputs = chunks.into_iter().map(EmbeddingInput::Text).collect();
                    let response = executor
                        .execute(state, inputs, settings.clone(), None)
                        .await;
                    let value = response_json(response, limits.max_decoded_bytes()).await?;
                    retain_output(&value, &mut output_bytes, &settings)?;
                    vectors.push(value);
                    parts = doc.parts;
                }
                Kind::Vision { entry, options } => {
                    // Partial native extraction must not be promoted to complete model context.
                    let text = doc.text(limits, true)?;
                    parts.push(
                        vision(
                            state,
                            entry,
                            options,
                            vec![PreparedPart::Text { text }],
                            &settings,
                            lease.clone(),
                            cancel.clone(),
                            None,
                        )
                        .await?,
                    );
                }
            }
        }
        guard.complete();
        let vector_only = parts.is_empty() && !vectors.is_empty();
        let document = ExtractedDocument { mime_type, parts };
        let mut item = document_item(document, settings.media.limits, chunk_size, return_text)?;
        item["model"] = json!(self.id);
        item["processing"] = json!(match &self.kind {
            Kind::Embedding { image: true, .. } => "image_embeddings",
            Kind::Embedding { image: false, .. } => "text_embeddings",
            Kind::Vision { .. } => "model_text",
        });
        if !vectors.is_empty() {
            item["embeddings"] = json!(vectors);
        }
        if vector_only {
            item["status"] = json!("ingested");
        }
        Ok(item)
    }
}

pub(super) fn document_item(
    document: ExtractedDocument,
    limits: cluaiz_core::media_limits::MediaLimits,
    chunk_size: usize,
    return_text: bool,
) -> Result<Value> {
    let complete = document.complete();
    let chunks = match document.text(limits, false) {
        Ok(text) => documents::chunker::SemanticChunker::chunk(&text, chunk_size)?,
        Err(_) if !complete => Vec::new(),
        Err(e) => return Err(e),
    };
    let mut item = json!({"status":if complete {"ingested"} else {"partial"}, "mime_type":document.mime_type,
        "chunk_size":chunk_size, "chunks_count":chunks.len()});
    if return_text {
        item["parts"] = serde_json::to_value(document.parts)?;
        item["chunks"] = json!(chunks);
    } else {
        item["parts"] = json!(document
            .parts
            .iter()
            .map(|p| json!({"page":p.page,"method":p.method,
        "omissions":p.omissions,"warnings":p.warnings,"ocr_confidence":p.ocr_confidence}))
            .collect::<Vec<_>>());
    }
    Ok(item)
}

async fn response_json(response: Response, limit: u64) -> Result<Value> {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::try_from(limit)?).await?;
    let value = serde_json::from_slice(&bytes)?;
    if !status.is_success() {
        return Err(ModelFailure {
            status,
            body: value,
        }
        .into());
    }
    Ok(value)
}
fn retain_output(value: &Value, total: &mut u64, settings: &ActiveUserSettings) -> Result<()> {
    *total = total
        .checked_add(serde_json::to_vec(value)?.len() as u64)
        .context("Ingest output size overflow")?;
    settings.media.limits.check_decoded_bytes(*total)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn vision(
    state: &Arc<AppState>,
    entry: &ModelRegistryEntry,
    options: &VisionSettings,
    mut content: Vec<PreparedPart>,
    settings: &ActiveUserSettings,
    lease: Option<MediaLease>,
    cancel: Arc<AtomicBool>,
    page: Option<u32>,
) -> Result<DocumentPart> {
    let instruction = match &options.system_instruction {
        Some(value) if !value.trim().is_empty() => value.clone(),
        _ => serde_json::from_str::<Value>(include_str!("../../../data/ingest.json"))?["instruction"]
            .as_str()
            .context("Ingest instruction resource is missing")?
            .to_owned(),
    };
    content.insert(0, PreparedPart::Text { text: instruction });
    let path = entry
        .primary_path()
        .context("Selected vision model is missing")?;
    state.host.ensure_loaded(&entry.id, &path, settings).await?;
    let tags = state.host.get_thinking_tags(None).await;
    let request = ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content,
        }],
        samplers: settings.samplers.clone(),
        think_mode: "off".into(),
        reasoning_effort: cluaiz_core::ThinkingBudget::Tokens(0),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
        tools: None,
    };
    let maximum = options
        .max_tokens
        .unwrap_or(crate::handlers::chat::DEFAULT_MAX_TOKENS);
    let dispatch = state
        .host
        .dispatch_prepared_with_progress(
            request,
            maximum,
            &path,
            settings,
            Some(cancel),
            None,
            None,
            lease,
            None,
        )
        .await;
    let raw = match dispatch {
        HostResponse::Completed(text) => text,
        HostResponse::Error(error) => bail!("{error}"),
        HostResponse::TokenStream(mut receiver) => {
            let mut text = String::new();
            // EngineHost signals normal completion by closing this raw token channel.
            // HTTP SSE terminators belong to its API adapter, not this typed owner.
            while let Some(token) = receiver.recv().await {
                if let Some(error) = cluaiz_runtime::stream_error(&token) {
                    bail!("{error}");
                }
                settings.media.limits.check_decoded_bytes(
                    text.len()
                        .checked_add(token.len())
                        .context("Model output size overflow")? as u64,
                )?;
                text.push_str(&token);
            }
            text
        }
    };
    let (_, text, _) = cluaiz_models::StructuralDNA::separate_reasoning(
        &raw,
        tags.0.as_deref(),
        tags.1.as_deref(),
    );
    if text.trim().is_empty() {
        bail!("Selected vision model returned no text");
    }
    let clipped = state
        .host
        .last_context()
        .await
        .is_some_and(|context| matches!(context.stop, "limit" | "cancel"));
    Ok(DocumentPart { page, method:"model_text", text,
        omissions: if clipped { vec!["model_generation_incomplete"] } else { Vec::new() },
        warnings: vec!["Generated recognition may be inaccurate; this is selected-model output, not native OCR".into()],
        ocr_confidence:None })
}
