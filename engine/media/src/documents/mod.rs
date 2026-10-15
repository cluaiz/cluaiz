//! Model-independent document extraction shared by chat, ingestion and CLI consumers.

pub mod chunker;
mod docx;
pub mod ocr;
mod pdf;
pub mod pdf_render;

use crate::formats::{DocumentKind, MediaFormat};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use serde::Serialize;

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
        DocumentKind::Docx => docx::extract(bytes, limits)?,
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
            let text = std::str::from_utf8(bytes).context("Document text must be UTF-8")?;
            limits.check_decoded_bytes(u64::try_from(text.len())?)?;
            if text.trim().is_empty() {
                bail!("Document text is empty");
            }
            vec![DocumentPart {
                page: None,
                method: "utf8",
                text: text.to_owned(),
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
