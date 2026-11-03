//! Model-independent document extraction shared by chat, ingestion and CLI consumers.

pub mod chunker;
mod doc;
mod docx;
pub mod ocr;
pub mod office;
mod pdf;
pub mod pdf_render;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_confidence: Option<i32>,
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
            bail!("Document contains no extracted text; OCR or vision may be required");
        }
        Ok(text)
    }
}

/// Signature/registered MIME evidence determines the provider, never a model name.
pub fn extract(
    bytes: &[u8],
    format: &MediaFormat,
    limits: MediaLimits,
) -> Result<ExtractedDocument> {
    extract_with_assets(bytes, format, limits, None)
}

/// Explicit assets are useful for embedded callers and isolated acceptance tests.
/// None resolves packaged assets lazily, only when recognition is actually needed.
pub fn extract_with_assets(
    bytes: &[u8],
    format: &MediaFormat,
    limits: MediaLimits,
    assets: Option<&crate::native_assets::NativeAssets>,
) -> Result<ExtractedDocument> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    let parts = match format.document_kind(bytes)? {
        DocumentKind::Pdf => {
            let mut parts = pdf::extract(bytes, limits)?;
            if parts.iter().any(|part| !part.omissions.is_empty()) {
                let owned;
                let assets = match assets {
                    Some(assets) => Ok(assets),
                    None => {
                        owned = crate::native_assets::NativeAssets::from_environment();
                        owned.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))
                    }
                };
                match assets {
                    Ok(assets) => pdf::recognize_pages(bytes, &mut parts, limits, assets)?,
                    Err(error) => {
                        for part in &mut parts {
                            if !part.omissions.is_empty() {
                                part.warnings
                                    .push(format!("Native OCR/PDF assets unavailable: {error:#}"));
                            }
                        }
                    }
                }
            }
            parts
        }
        DocumentKind::Doc => match doc::extract(bytes, limits) {
            Ok(parts) => parts,
            // A compound file that is a legacy workbook or presentation, not a Word document
            Err(error) => match office::segments(bytes, office::OfficeKind::LegacyCompound, limits) {
                Ok(segments) => with_pictures(segments, "office_markdown", Vec::new(), limits, assets),
                Err(_) => return Err(error),
            },
        },
        DocumentKind::Docx => {
            let body = docx::extract(bytes, limits)?;
            with_pictures(body.segments, "docx_xml", body.omissions, limits, assets)
        }
        DocumentKind::Office(kind) => {
            with_pictures(office::segments(bytes, kind, limits)?, "office_markdown", Vec::new(), limits, assets)
        }
        DocumentKind::Image => {
            let image = crate::image::decode_image(bytes, limits)?;
            let mut part = DocumentPart {
                page: None,
                method: "native_ocr",
                text: String::new(),
                omissions: Vec::new(),
                warnings: Vec::new(),
                ocr_confidence: None,
            };
            let result = (|| {
                let owned;
                let assets = match assets {
                    Some(assets) => assets,
                    None => {
                        owned = crate::native_assets::NativeAssets::from_environment()?;
                        &owned
                    }
                };
                ocr::recognize(&image, limits, assets)
            })();
            match result {
                Ok(result) => {
                    part.text = result.text;
                    part.ocr_confidence = Some(result.confidence);
                    part.warnings.push(OCR_TEXT_SCOPE.into());
                }
                Err(error) => {
                    part.omissions.push("native_ocr_unavailable_or_failed");
                    part.warnings.push(format!("{error:#}"));
                }
            }
            vec![part]
        }
        DocumentKind::Text => {
            let (text, method) = text::decode(bytes, limits)?;
            vec![DocumentPart {
                page: None,
                method,
                text,
                omissions: Vec::new(),
                warnings: Vec::new(),
                ocr_confidence: None,
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

/// Text segments become parts; each picture is read with OCR (its text follows its label), and
/// a picture with no text, in a format that cannot be decoded, or without OCR installed gets a
/// label that says so. An unread picture is marked in the text, so it is not an omission: the
/// model knows a picture was there and that its content is not in the text.
fn with_pictures(
    segments: Vec<Segment>,
    method: &'static str,
    omissions: Vec<&'static str>,
    limits: MediaLimits,
    assets: Option<&crate::native_assets::NativeAssets>,
) -> Vec<DocumentPart> {
    let mut parts: Vec<DocumentPart> = Vec::new();
    let mut owned: Option<Result<crate::native_assets::NativeAssets>> = None;
    let mut number = 0usize;
    let part = |method, text, warnings| DocumentPart { page: None, method, text, omissions: Vec::new(), warnings, ocr_confidence: None };
    for segment in segments {
        match segment {
            Segment::Text(text) => {
                if !text.trim().is_empty() {
                    parts.push(part(method, text, Vec::new()));
                }
            }
            Segment::Unread(why) => {
                number += 1;
                parts.push(part(method, picture_label(number, &format!("{why}; its content is not in this text")), Vec::new()));
            }
            Segment::Image(data) => {
                number += 1;
                let image = match crate::image::decode_image(&data, limits) {
                    Ok(image) => image,
                    Err(error) => {
                        parts.push(part("embedded_image_ocr", picture_label(number, "a picture in a format that cannot be read; its content is not in this text"), vec![format!("Picture {number}: {error:#}")]));
                        continue;
                    }
                };
                let assets = match assets {
                    Some(assets) => Ok(assets),
                    None => owned
                        .get_or_insert_with(crate::native_assets::NativeAssets::from_environment)
                        .as_ref()
                        .map_err(|e| anyhow::anyhow!("{e:#}")),
                };
                let mut picture = part("embedded_image_ocr", String::new(), Vec::new());
                match assets.and_then(|assets| ocr::recognize(&image, limits, assets)) {
                    Ok(found) => {
                        picture.text = format!("{}{}
", picture_label(number, "a picture; the text read from it follows"), found.text.trim());
                        picture.ocr_confidence = Some(found.confidence);
                        picture.warnings.push(OCR_TEXT_SCOPE.into());
                    }
                    Err(error) if error.to_string() == ocr::NO_TEXT => {
                        picture.text = picture_label(number, "a picture with no text in it; what it shows is not in this text");
                    }
                    Err(error) => {
                        picture.text = picture_label(number, "a picture that was not read; its content is not in this text (text recognition is not installed)");
                        picture.warnings.push(format!("Picture {number}: {error:#}"));
                    }
                }
                parts.push(picture);
            }
        }
    }
    if let Some(first) = parts.first_mut() {
        first.omissions = omissions;
    }
    parts
}

pub(super) const OCR_TEXT_SCOPE: &str = "OCR returns recognized text; it does not interpret diagrams, image meaning or certify recognition accuracy";

/// Call after request workers stop, before exiting an embedding host/CLI. Each
/// provider lock waits for native use to finish, then runs its actual destructor.
pub fn release_native_providers() -> Result<()> {
    let ocr = ocr::unload();
    let pdf = pdf_render::unload();
    ocr?;
    pdf
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
