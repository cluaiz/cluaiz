//! Page-by-page PDF text extraction using the existing project's pdf-extract provider.

use super::DocumentPart;
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use std::io::{self, Write};

struct LimitedText {
    bytes: Vec<u8>,
    remaining: u64,
}
impl Write for LimitedText {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let size = u64::try_from(bytes.len()).map_err(io::Error::other)?;
        if size > self.remaining {
            return Err(io::Error::other(
                "PDF extracted text exceeds the decoded byte limit",
            ));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        self.remaining -= size;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn extract(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
    // The upstream parser has panic paths. Do not turn malformed input into process failure.
    // This is not a total-process allocation bound: parser-owned PDF allocations remain separate.
    std::panic::catch_unwind(|| extract_inner(bytes, limits)).map_err(|_| {
        anyhow::anyhow!("PDF parser rejected malformed or unsupported document content")
    })?
}

fn extract_inner(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
    let doc = pdf_extract::Document::load_mem(bytes).context("Cannot parse PDF document")?;
    if doc.is_encrypted() {
        bail!("Encrypted PDF extraction is not supported; provide an unencrypted document");
    }
    let pages = doc.get_pages();
    if pages.is_empty() {
        bail!("PDF has no pages");
    }
    let overhead = pages
        .len()
        .checked_mul(std::mem::size_of::<DocumentPart>())
        .context("PDF page metadata size overflow")?;
    limits.check_decoded_bytes(u64::try_from(overhead)?)?;
    let mut remaining = limits
        .max_decoded_bytes()
        .checked_sub(u64::try_from(overhead)?)
        .context("PDF page metadata exceeds the byte limit")?;
    let mut parts = Vec::new();
    for (page, page_id) in &pages {
        let mut writer = LimitedText {
            bytes: Vec::new(),
            remaining,
        };
        let mut output = pdf_extract::PlainTextOutput::new(&mut writer as &mut dyn Write);
        pdf_extract::output_doc_page(&doc, &mut output, *page)
            .map_err(|error| anyhow::anyhow!("Cannot extract PDF page {page}: {error:?}"))?;
        remaining = writer.remaining;
        let text =
            String::from_utf8(writer.bytes).context("PDF extraction produced invalid UTF-8")?;
        let mut omissions = if text.trim().is_empty() {
            vec!["page_has_no_extractable_text_ocr_or_vision_unavailable"]
        } else {
            Vec::new()
        };
        // Conservatively report image/form resources instead of claiming that their
        // visible contents were recognized by text extraction. Includes inherited resources.
        let (direct, inherited) = doc.get_page_resources(*page_id)?;
        let mut visual = direct.is_some_and(|resources| resources.has(b"XObject"));
        for id in inherited {
            visual |= doc.get_dictionary(id)?.has(b"XObject");
        }
        if visual {
            omissions.push("page_visual_resources_not_fully_extracted");
        }
        parts.try_reserve(1)?;
        parts.push(DocumentPart {
            page: Some(*page),
            method: "pdf_text",
            text,
            omissions,
            warnings: Vec::new(),
            ocr_confidence: None,
        });
    }
    Ok(parts)
}

/// Retain existing embedded text when recognizing visual resources on the same
/// page. OCR failures stay local to their physical page and preserve prior text.
pub(super) fn recognize_pages(
    bytes: &[u8],
    parts: &mut [DocumentPart],
    limits: MediaLimits,
    assets: &crate::native_assets::NativeAssets,
) -> Result<()> {
    let metadata = u64::try_from(std::mem::size_of_val(parts))?;
    let mut retained = parts.iter().try_fold(metadata, |total, part| {
        total
            .checked_add(part.text.len() as u64)
            .context("PDF retained text size overflow")
    })?;
    let result = super::pdf_render::with_document(bytes, assets, |document| {
        for part in parts.iter_mut().filter(|part| !part.omissions.is_empty()) {
            let remaining = limits
                .max_decoded_bytes()
                .checked_sub(retained)
                .filter(|bytes| *bytes > 0)
                .context("PDF OCR decoded budget is exhausted")?;
            let page_limits = MediaLimits::new(limits.max_input_bytes(), remaining)?;
            let result = (|| {
                let image = document
                    .render_page(part.page.context("PDF page number missing")?, page_limits)?;
                super::ocr::recognize(&image, page_limits, assets)
            })();
            match result {
                Ok(result) => {
                    let old_len = part.text.len();
                    if part.text.trim().is_empty() {
                        part.text = result.text;
                        part.method = "pdf_ocr";
                    } else {
                        let part_limits = MediaLimits::new(
                            limits.max_input_bytes(),
                            limits
                                .max_decoded_bytes()
                                .checked_sub(
                                    retained
                                        .checked_sub(u64::try_from(old_len)?)
                                        .context("PDF retained text accounting underflow")?,
                                )
                                .context("PDF text assembly budget exhausted")?,
                        )?;
                        super::append(
                            &mut part.text,
                            "\n[OCR recognized page text]\n",
                            part_limits,
                        )?;
                        super::append(&mut part.text, &result.text, part_limits)?;
                        part.method = "pdf_text_and_ocr";
                    }
                    retained = retained
                        .checked_sub(u64::try_from(old_len)?)
                        .and_then(|retained| retained.checked_add(part.text.len() as u64))
                        .context("PDF OCR text size overflow")?;
                    limits.check_decoded_bytes(retained)?;
                    part.ocr_confidence = Some(result.confidence);
                    part.omissions.clear();
                    part.warnings.push(super::OCR_TEXT_SCOPE.into());
                }
                Err(error) => part
                    .warnings
                    .push(format!("Native page OCR failed: {error:#}")),
            }
        }
        Ok(())
    });
    if let Err(error) = result {
        for part in parts.iter_mut().filter(|part| !part.omissions.is_empty()) {
            part.warnings
                .push(format!("Native PDF rendering/OCR unavailable: {error:#}"));
        }
    }
    Ok(())
}
