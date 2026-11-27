//! Model-independent document extraction shared by chat, ingestion and CLI consumers.

pub mod chunker;
mod doc;
mod docx;
pub mod office;
mod pdf;
pub mod pdf_render;
pub mod pdfium_fetch;
mod text;

use crate::formats::{DocumentKind, MediaFormat};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use serde::Serialize;

/// One piece of a document in reading order: its text, an embedded picture (its file bytes), or
/// a visual that could not be read, and why
#[derive(Debug)]
pub enum Segment {
    Text(String),
    Image(Vec<u8>),
    Unread(&'static str),
}

/// Default maximum UTF-8 bytes per semantic chunk, shared by API and CLI.
pub const DEFAULT_CHUNK_BYTES: usize = 512;

/// One extracted part. Only PDFs carry a physical page number.
#[derive(Debug, Serialize)]
pub struct DocumentPart {
    pub page: Option<u32>,
    pub method: &'static str,
    pub text: String,
    pub omissions: Vec<&'static str>,
    pub warnings: Vec<String>,
}

/// Omission of a PDF page that has no text layer: it is read only by rendering it for a vision model
pub const PAGE_WITHOUT_TEXT: &str = "page_has_no_text_layer_needs_vision_model";
/// Omission of an image given to extraction without a vision model
pub const IMAGE_NEEDS_VISION: &str = "image_needs_vision_model";

/// The page numbers among PDF parts that have no text layer, in order: the only pages that are
/// rendered and sent to a vision model. Pages with text are read from their text layer.
pub fn pages_without_text(parts: &[DocumentPart]) -> Vec<u32> {
    parts
        .iter()
        .filter(|part| part.omissions.contains(&PAGE_WITHOUT_TEXT))
        .filter_map(|part| part.page)
        .collect()
}

/// Page-by-page text of a PDF, with pages that have no text layer marked by `PAGE_WITHOUT_TEXT`.
/// No renderer and no download: only the text layer is read.
pub fn pdf_pages(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    pdf::extract(bytes, limits)
}

/// Missing recognition is represented explicitly rather than fabricated text.
#[derive(Debug, Serialize)]
pub struct ExtractedDocument {
    pub mime_type: String,
    pub parts: Vec<DocumentPart>,
}

impl ExtractedDocument {
    pub fn complete(&self) -> bool {
        !self.parts.is_empty() && self.parts.iter().all(|part| part.omissions.is_empty())
    }

    /// Ordered real content with page provenance. Chat refuses partial extraction.
    pub fn text(&self, limits: MediaLimits, require_complete: bool) -> Result<String> {
        if require_complete && !self.complete() {
            let reasons = self
                .parts
                .iter()
                .flat_map(|part| &part.warnings)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            bail!("Document extraction is partial; omitted content cannot be sent as complete model input. {reasons}");
        }
        let mut text = String::new();
        for part in &self.parts {
            if part.text.trim().is_empty() {
                continue;
            }
            if let Some(page) = part.page {
                append(&mut text, &format!("\n[Document page {page}]\n"), limits)?;
            }
            append(&mut text, &part.text, limits)?;
            append(&mut text, "\n", limits)?;
        }
        if text.trim().is_empty() {
            bail!("Document contains no extracted text; its pages or pictures need a vision model to be read");
        }
        Ok(text)
    }
}

/// Signature/registered MIME evidence determines the provider, never a model name. Extraction
/// reads text only and runs no model: PDF pages without a text layer and images come back as
/// omissions that say a vision model is needed; Word/Office pictures are labelled as not read.
pub fn extract(
    bytes: &[u8],
    format: &MediaFormat,
    limits: MediaLimits,
) -> Result<ExtractedDocument> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    let parts = match format.document_kind(bytes)? {
        DocumentKind::Pdf => pdf::extract(bytes, limits)?,
        DocumentKind::Doc => match doc::extract(bytes, limits) {
            Ok(parts) => parts,
            // A compound file that is a legacy workbook or presentation, not a Word document
            Err(error) => match office::segments(bytes, office::OfficeKind::LegacyCompound, limits) {
                Ok(segments) => with_pictures(segments, "office_markdown", Vec::new()),
                Err(_) => return Err(error),
            },
        },
        DocumentKind::Docx => {
            let body = docx::extract(bytes, limits)?;
            with_pictures(body.segments, "docx_xml", body.omissions)
        }
        DocumentKind::Office(kind) => {
            with_pictures(office::segments(bytes, kind, limits)?, "office_markdown", Vec::new())
        }
        DocumentKind::Image => {
            // Decoding proves it is a readable image; its content needs a vision model
            crate::image::decode_image(bytes, limits)?;
            vec![DocumentPart {
                page: None,
                method: "image",
                text: String::new(),
                omissions: vec![IMAGE_NEEDS_VISION],
                warnings: vec!["An image is read by a vision model; text extraction alone cannot read it".into()],
            }]
        }
        DocumentKind::Text => {
            let (text, method) = text::decode(bytes, limits)?;
            vec![DocumentPart {
                page: None,
                method,
                text,
                omissions: Vec::new(),
                warnings: Vec::new(),
            }]
        }
    };
    Ok(ExtractedDocument {
        mime_type: format.mime_type.clone(),
        parts,
    })
}

/// The text and pictures of a Word or Office document in reading order, for a model that reads
/// pictures itself; None for other kinds
pub fn segments(bytes: &[u8], format: &MediaFormat, limits: MediaLimits) -> Result<Option<Vec<Segment>>> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    Ok(match format.document_kind(bytes)? {
        DocumentKind::Docx => Some(docx::extract(bytes, limits)?.segments),
        DocumentKind::Office(kind) => Some(office::segments(bytes, kind, limits)?),
        _ => None,
    })
}

/// What a model reading text is told about the picture numbered `number`, without the picture
pub fn picture_label(number: usize, note: &str) -> String {
    format!("
[Picture {number}: {note}]
")
}

/// Text segments become parts; each picture gets a label in its place saying it was not read (a
/// model that reads pictures gets them through `segments` instead). An unread picture is marked in
/// the text, so it is not an omission: the model knows a picture was there and that its content is
/// not in the text.
fn with_pictures(
    segments: Vec<Segment>,
    method: &'static str,
    omissions: Vec<&'static str>,
) -> Vec<DocumentPart> {
    let mut parts: Vec<DocumentPart> = Vec::new();
    let mut number = 0usize;
    let part = |method, text| DocumentPart { page: None, method, text, omissions: Vec::new(), warnings: Vec::new() };
    for segment in segments {
        match segment {
            Segment::Text(text) => {
                if !text.trim().is_empty() {
                    parts.push(part(method, text));
                }
            }
            Segment::Unread(why) => {
                number += 1;
                parts.push(part(method, picture_label(number, &format!("{why}; its content is not in this text"))));
            }
            Segment::Image(_) => {
                number += 1;
                parts.push(part(method, picture_label(number, "a picture that was not read; its content is not in this text (reading pictures needs a vision model)")));
            }
        }
    }
    if let Some(first) = parts.first_mut() {
        first.omissions = omissions;
    }
    parts
}

/// Call after request workers stop, before exiting an embedding host/CLI. The PDF renderer lock
/// waits for native use to finish, then runs its actual destructor.
pub fn release_native_providers() -> Result<()> {
    pdf_render::unload()
}

/// Checks before text growth; callers also account for retained request/batch data.
pub(super) fn append(output: &mut String, text: &str, limits: MediaLimits) -> Result<()> {
    let size = output
        .len()
        .checked_add(text.len())
        .context("Document text size overflow")?;
    limits.check_decoded_bytes(u64::try_from(size)?)?;
    output.try_reserve_exact(text.len())?;
    output.push_str(text);
    Ok(())
}
