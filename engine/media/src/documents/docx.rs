//! Bounded main-body WordprocessingML extraction. No archive files are written to disk.

use super::{append, DocumentPart};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::io::{Cursor, Read};

const BODY_PATH: &str = "word/document.xml";
const MAX_XML_DEPTH: usize = 128;
const WORD_NAMESPACE: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD_NAMESPACE: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";

pub(super) fn extract(bytes: &[u8], limits: MediaLimits) -> Result<Vec<DocumentPart>> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).context("Cannot parse DOCX ZIP container")?;
    let mut omissions = Vec::new();
    if archive
        .file_names()
        .flatten()
        .any(|name| name.starts_with("word/header") || name.starts_with("word/footer"))
    {
        omissions.push("header_footer_content_not_extracted");
    }
    let body = archive
        .by_name(BODY_PATH)
        .context("ZIP is not a supported DOCX: main body is missing")?;
    limits.check_decoded_bytes(body.size())?;
    let mut xml = Vec::new();
    body.take(limits.max_decoded_bytes().saturating_add(1))
        .read_to_end(&mut xml)?;
    limits.check_decoded_bytes(u64::try_from(xml.len())?)?;
    std::str::from_utf8(&xml).context("DOCX main body must use UTF-8 XML")?;
    let mut reader = NsReader::from_reader(xml.as_slice());
    let mut text = String::new();
    let mut depth = 0usize;
    let mut text_depth = None;
    let mut body_seen = false;
    let mut root_seen = false;
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        let word = matches!(namespace, ResolveResult::Bound(namespace)
            if namespace.as_ref() == WORD_NAMESPACE || namespace.as_ref() == STRICT_WORD_NAMESPACE);
        match event {
            Event::Start(element) => {
                depth = depth.checked_add(1).context("DOCX XML depth overflow")?;
                if depth > MAX_XML_DEPTH {
                    bail!("DOCX XML nesting limit exceeded");
                }
                let name = element.local_name();
                if depth == 1 {
                    if root_seen {
                        bail!("DOCX XML has multiple document roots");
                    }
                    root_seen = true;
                }
                if depth == 1 && (!word || name.as_ref() != b"document") {
                    bail!("DOCX main XML has no WordprocessingML document root");
                }
                if !word {
                    continue;
                }
                match name.as_ref() {
                    b"body" if depth == 2 => body_seen = true,
                    b"t" => text_depth = Some(depth),
                    b"drawing" | b"object" | b"pict" => {
                        if !omissions.contains(&"embedded_visual_content_not_extracted") {
                            omissions.push("embedded_visual_content_not_extracted");
                        }
                    }
                    _ => {}
                }
            }
            Event::End(element) => {
                if text_depth == Some(depth) {
                    text_depth = None;
                }
                depth = depth.checked_sub(1).context("Unbalanced DOCX XML")?;
                if !word {
                    continue;
                }
                match element.local_name().as_ref() {
                    b"p" | b"tr" => append(&mut text, "\n", limits)?,
                    b"tc" => append(&mut text, "\t", limits)?,
                    _ => {}
                }
            }
            Event::Empty(element) if word => match element.local_name().as_ref() {
                b"drawing" | b"object" | b"pict" => {
                    if !omissions.contains(&"embedded_visual_content_not_extracted") {
                        omissions.push("embedded_visual_content_not_extracted");
                    }
                }
                b"tab" => append(&mut text, "\t", limits)?,
                b"br" | b"cr" => append(&mut text, "\n", limits)?,
                _ => {}
            },
            Event::Text(value) if text_depth.is_some() => {
                append(&mut text, &value.xml11_content()?, limits)?
            }
            Event::CData(value) if text_depth.is_some() => {
                append(&mut text, &value.decode()?, limits)?
            }
            Event::GeneralRef(value) if text_depth.is_some() => {
                let entity = format!("&{};", value.decode()?);
                append(&mut text, &quick_xml::escape::unescape(&entity)?, limits)?;
            }
            Event::DocType(_) => bail!("DOCX XML document types/entities are not supported"),
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        bail!("Truncated DOCX XML");
    }
    if !body_seen {
        bail!("DOCX main XML body is missing");
    }
    if text.trim().is_empty() {
        bail!("DOCX has no extractable main-body text");
    }
    Ok(vec![DocumentPart {
        page: None,
        method: "docx_xml",
        text,
        omissions,
        warnings: Vec::new(),
        ocr_confidence: None,
    }])
}
