//! Ordered source-to-buffer preparation. This layer never invokes an inference model.

use crate::{audio::decode_audio, image::decode_image, source::SourceResolver};
use anyhow::{Context, Result};
use cluaiz_core::{
    chat::{ChatEnvelope, ChatMessage},
    contracts::EngineError,
    media::{InputPart, PreparedPart},
    media_limits::MediaLimits,
    processing::{AttachmentRef, ProcessingStage as Stage, ProcessingState as State},
};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Cancels detached preparation when its awaiting caller disappears.
pub struct PreparationGuard {
    signal: Arc<AtomicBool>,
    active: bool,
}
impl PreparationGuard {
    /// Retain the request's shared signal until preparation completes or is dropped.
    pub fn new(signal: Arc<AtomicBool>) -> Self {
        Self {
            signal,
            active: true,
        }
    }

    /// Normal completion leaves the same signal usable by subsequent native generation.
    pub fn complete(&mut self) {
        self.active = false;
    }
}
impl Drop for PreparationGuard {
    fn drop(&mut self) {
        if self.active {
            self.signal.store(true, Ordering::Release);
        }
    }
}

/// Prepares an entire request in order, retaining decoded data and generation options.
/// The canonical-path authorization callback is supplied by the calling request boundary.
pub async fn prepare_chat(
    request: ChatEnvelope<Vec<InputPart>>,
    resolver: &SourceResolver,
    authorize_local: impl Fn(&Path) -> Result<()> + Send + Sync,
) -> Result<ChatEnvelope<Vec<PreparedPart>>> {
    let mut guard = PreparationGuard::new(resolver.cancellation());
    let result = request
        .try_map_messages(|messages| async move {
            // Known unavailable providers fail before any source fetch or decoding.
            if messages
                .iter()
                .flat_map(|message| &message.content)
                .any(|part| matches!(part, InputPart::Video { .. }))
            {
                tokio::task::spawn_blocking(crate::video::attachment_formats)
                    .await?
                    .map_err(|error| {
                        EngineError::UnsupportedModality(format!(
                            "Video decoder unavailable: {error:#}"
                        ))
                    })?;
            }
            let limits = resolver.limits();
            let mut retained = 0_u64;
            let mut prepared = Vec::with_capacity(messages.len());
            for (message_index, message) in messages.into_iter().enumerate() {
                let mut parts = Vec::with_capacity(message.content.len());
                for (part_index, part) in message.content.into_iter().enumerate() {
                    if resolver.cancellation().load(Ordering::Acquire) {
                        anyhow::bail!("Media preparation cancelled");
                    }
                    let target = resolver
                        .progress
                        .as_ref()
                        .map(|p| p.source_attachment(message_index, part_index))
                        .unwrap_or(AttachmentRef {
                            message_index,
                            part_index,
                            frame_index: None,
                        });
                    let is_video = matches!(part, InputPart::Video { .. });
                    let result = prepare_part(part, resolver, &authorize_local, retained, target)
                        .await
                        .with_context(|| {
                            format!("Cannot prepare message {message_index}, part {part_index}")
                        })?;
                    let mut frame = 0;
                    for result in result {
                        if let Some(progress) = resolver
                            .progress
                            .as_ref()
                            .filter(|_| !matches!(result, PreparedPart::Text { .. }))
                        {
                            let frame_index =
                                if is_video && matches!(result, PreparedPart::Image { .. }) {
                                    let index = frame;
                                    frame += 1;
                                    Some(index)
                                } else {
                                    None
                                };
                            progress.map_prepared(
                                message_index,
                                parts.len(),
                                AttachmentRef {
                                    frame_index,
                                    ..target
                                },
                            );
                        }
                        retained = retained
                            .checked_add(u64::try_from(result.retained_bytes())?)
                            .context("Prepared request byte count overflow")?;
                        limits.check_decoded_bytes(retained)?;
                        parts.push(result);
                    }
                }
                prepared.push(ChatMessage {
                    role: message.role,
                    content: parts,
                });
            }
            Ok(prepared)
        })
        .await;
    guard.complete();
    result
}

async fn prepare_part(
    part: InputPart,
    resolver: &SourceResolver,
    authorize_local: &(impl Fn(&Path) -> Result<()> + Send + Sync),
    retained: u64,
    target: AttachmentRef,
) -> Result<Vec<PreparedPart>> {
    let limits = resolver.limits();
    let (source, input_kind, video_audio) = match part {
        InputPart::Text { text } => {
            let next = retained
                .checked_add(u64::try_from(text.capacity())?)
                .context("Text byte count overflow")?;
            limits.check_decoded_bytes(next)?;
            return Ok(vec![PreparedPart::Text { text }]);
        }
        InputPart::Image { source } => (source, Some(cluaiz_core::Modality::Image), false),
        InputPart::Audio { source } => (source, Some(cluaiz_core::Modality::Audio), false),
        InputPart::Document { source } => (source, Some(cluaiz_core::Modality::Text), false),
        InputPart::Video {
            source,
            include_audio,
        } => (source, None, include_audio),
    };
    let source = resolver.resolve(&source, authorize_local).await?;
    prepare_resolved(source, resolver, input_kind, video_audio, retained, target).await
}

/// Human-selected files use the registered content detector and the same bounded decoders.
/// This does not execute files, and authorization runs before their contents are read.
pub async fn prepare_attachment(
    input: &str,
    resolver: &SourceResolver,
    retained: u64,
    supported: &[cluaiz_core::Modality],
    authorize_local: impl Fn(&Path) -> Result<()> + Send + Sync,
) -> Result<Vec<PreparedPart>> {
    if resolver.cancellation().load(Ordering::Acquire) {
        anyhow::bail!("Media preparation cancelled");
    }
    let source = resolver.resolve(input, authorize_local).await?;
    let mime = source.format.mime_type.as_str();
    let kind = if mime.starts_with("image/") {
        Some(cluaiz_core::Modality::Image)
    } else if mime.starts_with("audio/") {
        Some(cluaiz_core::Modality::Audio)
    } else if mime.starts_with("video/") {
        None
    } else {
        Some(cluaiz_core::Modality::Text)
    };
    let required = kind.unwrap_or(cluaiz_core::Modality::Image);
    if !supported.contains(&required) {
        anyhow::bail!(
            "Selected model does not support {} input",
            source.format.mime_type
        );
    }
    prepare_resolved(
        source,
        resolver,
        kind,
        supported.contains(&cluaiz_core::Modality::Audio),
        retained,
        AttachmentRef {
            message_index: 0,
            part_index: 0,
            frame_index: None,
        },
    )
    .await
}

async fn prepare_resolved(
    source: crate::source::ResolvedSource,
    resolver: &SourceResolver,
    input_kind: Option<cluaiz_core::Modality>,
    video_audio: bool,
    retained: u64,
    target: AttachmentRef,
) -> Result<Vec<PreparedPart>> {
    let limits = resolver.limits();
    let remaining = limits
        .max_decoded_bytes()
        .checked_sub(retained)
        .filter(|remaining| *remaining > 0)
        .context("Prepared request has exhausted its decoded byte budget")?;
    let decode_limits = MediaLimits::new(limits.max_input_bytes(), remaining)?;
    if let Some(p) = &resolver.progress {
        p.emit(Stage::Reading, State::Running, Some(target), None);
    }
    let bytes = source.read_bytes(limits).await?;
    let format = source.format.clone();
    // From here on the worker owns its bytes. Temporary source files can be dropped;
    // prepared buffers own their data through model use and cloned continuations.
    drop(source);
    if let Some(p) = &resolver.progress {
        p.emit(Stage::Reading, State::Completed, Some(target), None);
        p.emit(Stage::Decoding, State::Running, Some(target), None);
    }
    let video_settings = resolver.video_settings();
    let document_images = resolver.document_images();
    let cancellation = resolver.cancellation();
    let decoded = tokio::task::spawn_blocking(move || match input_kind {
        Some(cluaiz_core::Modality::Image) => {
            decode_image(&bytes, decode_limits).map(|image| vec![PreparedPart::Image { image }])
        }
        Some(cluaiz_core::Modality::Audio) => {
            decode_audio(bytes, decode_limits).map(|audio| vec![PreparedPart::Audio { audio }])
        }
        Some(cluaiz_core::Modality::Text) => {
            if document_images && format.document_kind(&bytes)? == crate::formats::DocumentKind::Pdf
            {
                return prepare_pdf_pages(&bytes, decode_limits, &cancellation);
            }
            let document = crate::documents::extract(&bytes, &format, decode_limits)?;
            let text = document.text(decode_limits, true)?;
            Ok(vec![PreparedPart::Text { text }])
        }
        None => {
            if !format.mime_type.starts_with("video/") {
                anyhow::bail!("Video attachment is not a recognized video container");
            }
            let video = crate::video::decode(
                bytes,
                decode_limits,
                video_settings,
                video_audio,
                cancellation,
            )?;
            let mut parts = Vec::new();
            for frame in video.frames {
                parts.push(PreparedPart::Text {
                    text: format!(
                        "Video frame at {:.3} seconds:\n",
                        frame.timestamp_us as f64 / 1_000_000.0
                    ),
                });
                parts.push(PreparedPart::Image { image: frame.image });
            }
            if let Some(audio) = video.audio {
                parts.push(PreparedPart::Text {
                    text: "Audio track for the preceding video:\n".into(),
                });
                parts.push(PreparedPart::Audio { audio });
            }
            Ok(parts)
        }
    })
    .await??;
    if let Some(p) = &resolver.progress {
        p.emit(Stage::Decoding, State::Completed, Some(target), None);
    }
    Ok(decoded)
}

/// Reuse serialized PDFium; native bitmaps are temporary, retained RGB pages are bounded.
fn prepare_pdf_pages(
    bytes: &[u8],
    limits: MediaLimits,
    cancellation: &AtomicBool,
) -> Result<Vec<PreparedPart>> {
    let assets = crate::native_assets::NativeAssets::from_environment()?;
    crate::documents::pdf_render::with_document(bytes, &assets, |document| {
        let mut parts = Vec::new();
        let mut retained = 0_u64;
        for page in 1..=document.page_count()? {
            if cancellation.load(Ordering::Acquire) {
                anyhow::bail!("PDF preparation cancelled");
            }
            let label = PreparedPart::Text {
                text: format!("Document page {page}:\n"),
            };
            retained = retained
                .checked_add(label.retained_bytes() as u64)
                .context("PDF page label size overflow")?;
            let remaining = limits
                .max_decoded_bytes()
                .checked_sub(retained)
                .filter(|remaining| *remaining > 0)
                .context("PDF pages exceed the decoded byte budget")?;
            let page_limits = MediaLimits::new(limits.max_input_bytes(), remaining)?;
            let image = PreparedPart::Image {
                image: document.render_page(page, page_limits)?,
            };
            retained = retained
                .checked_add(image.retained_bytes() as u64)
                .context("PDF page buffer size overflow")?;
            limits.check_decoded_bytes(retained)?;
            parts.try_reserve(2)?;
            parts.extend([label, image]);
        }
        Ok(parts)
    })
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
