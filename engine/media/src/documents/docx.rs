//! Bounded main-body WordprocessingML extraction. No archive files are written to disk.
//! Pictures (`a:blip r:embed`, VML `v:imagedata r:id`) keep their place in the text and are read
//! from the package through the body's relationships; a drawing without one (a chart, a shape)
//! is marked as not read.

use super::{append, Segment};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use quick_xml::{events::BytesStart, events::Event, name::ResolveResult, reader::NsReader};
use std::io::{Cursor, Read};

const BODY_PATH: &str = "word/document.xml";
const RELS_PATH: &str = "word/_rels/document.xml.rels";
const MAX_XML_DEPTH: usize = 128;
const WORD_NAMESPACE: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD_NAMESPACE: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";
/// Where a drawing was in the text (U+FFFC, the object replacement character)
const OBJECT: char = '\u{FFFC}';

type Archive<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;

/// The body's text and pictures in order, and what was left out
pub(super) struct Body {
    pub segments: Vec<Segment>,
    pub omissions: Vec<&'static str>,
}

pub(super) fn extract(bytes: &[u8], limits: MediaLimits) -> Result<Body> {
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
    let xml = {
        let body = archive
            .by_name(BODY_PATH)
            .context("ZIP is not a supported DOCX: main body is missing")?;
        limits.check_decoded_bytes(body.size())?;
        let mut xml = Vec::new();
        body.take(limits.max_decoded_bytes().saturating_add(1))
            .read_to_end(&mut xml)?;
        xml
    };
    limits.check_decoded_bytes(u64::try_from(xml.len())?)?;
    std::str::from_utf8(&xml).context("DOCX main body must use UTF-8 XML")?;
    let mut reader = NsReader::from_reader(xml.as_slice());
    let mut text = String::new();
    let mut depth = 0usize;
    let mut text_depth = None;
    let mut body_seen = false;
    let mut root_seen = false;
    // Each drawing in order: the relationship id of its picture, None for one without
    let mut drawings: Vec<Option<String>> = Vec::new();
    // The depth of the drawing being read, and whether a picture was found in it
    let mut visual: Option<(usize, bool)> = None;
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
                    picture_in(&element, &mut visual, &mut drawings, &mut text, limits)?;
                    continue;
                }
                match name.as_ref() {
                    b"body" if depth == 2 => body_seen = true,
                    b"t" => text_depth = Some(depth),
                    b"drawing" | b"object" | b"pict" if visual.is_none() => {
                        visual = Some((depth, false))
                    }
                    _ => {}
                }
            }
            Event::End(element) => {
                if text_depth == Some(depth) {
                    text_depth = None;
                }
                if let Some((at, found)) = visual {
                    if at == depth {
                        if !found {
                            drawings.push(None);
                            append(&mut text, &OBJECT.to_string(), limits)?;
                        }
                        visual = None;
                    }
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
            Event::Empty(element) if !word => {
                picture_in(&element, &mut visual, &mut drawings, &mut text, limits)?
            }
            Event::Empty(element) => match element.local_name().as_ref() {
                // An empty drawing holds nothing to read
                b"drawing" | b"object" | b"pict" if visual.is_none() => {
                    drawings.push(None);
                    append(&mut text, &OBJECT.to_string(), limits)?;
                }
                b"tab" => append(&mut text, "\t", limits)?,
                b"br" | b"cr" => append(&mut text, "\n", limits)?,
                _ => {}
            },
            Event::Text(value) if text_depth.is_some() => append(
                &mut text,
                &value.xml11_content()?.replace(OBJECT, ""),
                limits,
            )?,
            Event::CData(value) if text_depth.is_some() => {
                append(&mut text, &value.decode()?.replace(OBJECT, ""), limits)?
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
    if text.trim_matches(|c: char| c.is_whitespace() || c == OBJECT).is_empty() && drawings.is_empty() {
        bail!("DOCX has no extractable main-body text");
    }
    let targets = if drawings.iter().any(Option::is_some) {
        relationships(&mut archive, limits)?
    } else {
        Vec::new()
    };
    let mut used = u64::try_from(text.len())?;
    let mut pieces = text.split(OBJECT);
    let mut segments = Vec::new();
    if let Some(first) = pieces.next() {
        segments.push(Segment::Text(first.to_string()));
    }
    for (drawing, piece) in drawings.into_iter().zip(pieces) {
        let target = drawing.map(|id| targets.iter().find(|(rid, _)| *rid == id).and_then(|(_, target)| target.clone()));
        segments.push(match target {
            Some(Some(path)) => match read_entry(&mut archive, &path, limits) {
                Ok(data) => {
                    used = used
                        .checked_add(u64::try_from(data.len())?)
                        .context("DOCX picture size overflow")?;
                    limits.check_decoded_bytes(used)?;
                    Segment::Image(data)
                }
                Err(_) => Segment::Unread("a picture that is missing from the file"),
            },
            Some(None) => Segment::Unread("a linked picture outside the document"),
            None => Segment::Unread("a drawing without a picture (a chart or a shape)"),
        });
        segments.push(Segment::Text(piece.to_string()));
    }
    Ok(Body { segments, omissions })
}

/// A picture element inside a drawing: its relationship id joins the text's drawings
fn picture_in(
    element: &BytesStart,
    visual: &mut Option<(usize, bool)>,
    drawings: &mut Vec<Option<String>>,
    text: &mut String,
    limits: MediaLimits,
) -> Result<()> {
    let Some((_, found)) = visual.as_mut() else {
        return Ok(());
    };
    if *found || !matches!(element.local_name().as_ref(), b"blip" | b"imagedata") {
        return Ok(());
    }
    for attribute in element.attributes().flatten() {
        if matches!(attribute.key.local_name().as_ref(), b"embed" | b"id") {
            let id = attribute.normalized_value(quick_xml::XmlVersion::default())?.to_string();
            *found = true;
            drawings.push(Some(id));
            append(text, &OBJECT.to_string(), limits)?;
            return Ok(());
        }
    }
    Ok(())
}

/// The body's relationships: id and package path (None for an external target)
fn relationships(archive: &mut Archive, limits: MediaLimits) -> Result<Vec<(String, Option<String>)>> {
    let Ok(xml) = read_entry(archive, RELS_PATH, limits) else {
        return Ok(Vec::new());
    };
    let mut reader = quick_xml::reader::Reader::from_reader(xml.as_slice());
    let mut found = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"Relationship" =>
            {
                let (mut id, mut target, mut external) = (None, None, false);
                for attribute in element.attributes().flatten() {
                    let value = attribute.normalized_value(quick_xml::XmlVersion::default())?.to_string();
                    match attribute.key.local_name().as_ref() {
                        b"Id" => id = Some(value),
                        b"Target" => target = Some(value),
                        b"TargetMode" => external = value.eq_ignore_ascii_case("External"),
                        _ => {}
                    }
                }
                if let (Some(id), Some(target)) = (id, target) {
                    found.push((id, (!external).then(|| package_path(&target))));
                }
            }
            Event::DocType(_) => bail!("DOCX relationships with document types are not supported"),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}

/// A relationship target as a path in the package (relative to `word/`, or from the root)
fn package_path(target: &str) -> String {
    let joined = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("word/{target}"),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// One entry of the package, read within the decoded byte budget
fn read_entry(archive: &mut Archive, path: &str, limits: MediaLimits) -> Result<Vec<u8>> {
    let entry = archive.by_name(path)?;
    limits.check_decoded_bytes(entry.size())?;
    let mut data = Vec::new();
    entry
        .take(limits.max_decoded_bytes().saturating_add(1))
        .read_to_end(&mut data)?;
    limits.check_decoded_bytes(u64::try_from(data.len())?)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    #[test]
    fn relationship_targets_resolve_inside_the_package() {
        assert_eq!(super::package_path("media/image1.png"), "word/media/image1.png");
        assert_eq!(super::package_path("../media/a.png"), "media/a.png");
        assert_eq!(super::package_path("/word/media/b.jpeg"), "word/media/b.jpeg");
        assert_eq!(super::package_path("../../../etc/x"), "etc/x");
    }
}
