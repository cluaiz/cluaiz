//! Legacy Word text extraction only; no Office process, rendering or model load.

use super::{append, DocumentPart};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use office_oxide::doc::DocDocument;
use std::io::Cursor;

pub(super) fn extract(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    let document = DocDocument::from_reader(Cursor::new(bytes))
        .context("Cannot extract legacy Word document")?;
    // Borrow the flat body: do not build the library's rich document/image model.
    let mut text = String::new();
    append(&mut text, document.plain_text_ref(), limits)?;
    for story in document.subdocuments() {
        if !story.text.trim().is_empty() {
            append(&mut text, "\n", limits)?;
            append(&mut text, &story.text, limits)?;
        }
    }
    if text.trim().is_empty() {
        bail!("Legacy Word document has no extractable text");
    }
    let mut warnings = document.warnings().to_vec();
    let omissions = if !document.text_complete() || !warnings.is_empty() {
        warnings.push("Legacy Word text could not be recovered completely".into());
        vec!["legacy_word_text_incomplete"]
    } else {
        Vec::new()
    };
    warnings.push(
        "Text extraction excludes graphics/layout and does not evaluate fields or macros".into(),
    );
    Ok(vec![DocumentPart {
        page: None,
        method: "doc_binary",
        text,
        omissions,
        warnings,
        ocr_confidence: None,
    }])
}
