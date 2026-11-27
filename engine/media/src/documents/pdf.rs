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

pub(crate) fn extract(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
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
        // A page without a text layer (a scanned page, or a page that is only a picture) is an
        // omission: its content needs rendering and a vision model, and is never passed off as read
        let scanned = text.trim().is_empty();
        let mut omissions = Vec::new();
        let mut warnings = Vec::new();
        if scanned {
            omissions.push(super::PAGE_WITHOUT_TEXT);
            warnings.push(format!("Page {page} has no text layer (a scanned page or a picture); reading it needs a vision model"));
        } else {
            // A page with text is read from its text layer. Pictures on it (image/form resources,
            // inherited ones too) are not read; say so instead of claiming they were
            let (direct, inherited) = doc.get_page_resources(*page_id)?;
            let mut visual = direct.is_some_and(|resources| resources.has(b"XObject"));
            for id in inherited {
                visual |= doc.get_dictionary(id)?.has(b"XObject");
            }
            if visual {
                warnings.push(format!("Page {page} also has pictures; only its text layer was read"));
            }
        }
        parts.try_reserve(1)?;
        parts.push(DocumentPart {
            page: Some(*page),
            method: "pdf_text",
            text,
            omissions,
            warnings,
        });
    }
    Ok(parts)
}
