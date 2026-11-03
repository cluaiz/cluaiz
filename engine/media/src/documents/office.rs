//! Spreadsheets and presentations (XLSX, PPTX, legacy XLS and PPT) through the pinned
//! `office_oxide` reader the legacy Word provider already uses: each sheet as a Markdown table,
//! each slide with its title, text, tables and speaker notes. Embedded pictures keep their place
//! in the flow (the reader's `[image-base64:…]` markers) and become [`Segment::Image`].

use super::Segment;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use cluaiz_core::media_limits::MediaLimits;
use office_oxide::ir_render::{ImageEmbed, MarkdownOptions};
use office_oxide::{Document, DocumentFormat};
use std::io::Cursor;

/// The reader's marker for an embedded picture in its Markdown
const IMAGE_MARKER: &str = "[image-base64:";

/// Which reader a file goes to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeKind {
    Spreadsheet,
    Presentation,
    /// A legacy compound file (XLS or PPT; a Word DOC goes to `doc.rs`): the reader finds which
    LegacyCompound,
}

/// The document's text and pictures in reading order. The text and the pictures' bytes count
/// against the decoded byte budget.
pub(super) fn segments(bytes: &[u8], kind: OfficeKind, limits: MediaLimits) -> Result<Vec<Segment>> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    let format = match kind {
        OfficeKind::Spreadsheet => DocumentFormat::Xlsx,
        OfficeKind::Presentation => DocumentFormat::Pptx,
        // Opened as one legacy format, the compound file's own streams decide (XLS or PPT)
        OfficeKind::LegacyCompound => DocumentFormat::Xls,
    };
    let document = Document::from_reader(Cursor::new(bytes.to_vec()), format).context("Cannot read the Office document")?;
    let markdown = document.to_markdown_with(MarkdownOptions { image_embed: ImageEmbed::Base64 });
    drop(document);
    let segments = split(&markdown, limits)?;
    if !segments.iter().any(|segment| matches!(segment, Segment::Text(text) if !text.trim().is_empty()) || matches!(segment, Segment::Image(_))) {
        bail!("The Office document has no text or pictures");
    }
    Ok(segments)
}

/// Markdown with picture markers, as text and picture segments in order
pub(super) fn split(markdown: &str, limits: MediaLimits) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut used = 0u64;
    let mut rest = markdown;
    let mut charge = |bytes: usize| -> Result<()> {
        used = used.checked_add(u64::try_from(bytes)?).context("Document size overflow")?;
        limits.check_decoded_bytes(used)?;
        Ok(())
    };
    while let Some(start) = rest.find(IMAGE_MARKER) {
        let after = &rest[start + IMAGE_MARKER.len()..];
        let Some(end) = after.find(']') else { break };
        let text = &rest[..start];
        if !text.is_empty() {
            charge(text.len())?;
            segments.push(Segment::Text(text.to_string()));
        }
        match base64::engine::general_purpose::STANDARD.decode(after[..end].trim()) {
            Ok(data) => {
                charge(data.len())?;
                segments.push(Segment::Image(data));
            }
            // Not a picture the reader wrote: kept as the text it is
            Err(_) => {
                let literal = &rest[start..start + IMAGE_MARKER.len() + end + 1];
                charge(literal.len())?;
                segments.push(Segment::Text(literal.to_string()));
            }
        }
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        charge(rest.len())?;
        segments.push(Segment::Text(rest.to_string()));
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_become_pictures_in_order_and_other_text_stays() {
        let png = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let text = format!("## Slide 1\n\nBefore\n\n[image-base64:{png}]\n\nAfter [image-base64:not base64!] end");
        let parts = split(&text, MediaLimits::default()).unwrap();
        assert!(matches!(&parts[0], Segment::Text(t) if t.contains("Before")));
        assert!(matches!(&parts[1], Segment::Image(d) if d == &[1, 2, 3]));
        assert!(matches!(&parts[2], Segment::Text(t) if t.contains("After")));
        assert!(matches!(&parts[3], Segment::Text(t) if t.contains("not base64")));
        assert!(split(&text, MediaLimits::new(1 << 20, 8).unwrap()).is_err());
    }
}
