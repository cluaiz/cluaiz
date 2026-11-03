//! Spreadsheets, presentations and pictures inside Word documents.
//! `cargo test -p cluaiz-media --test office_documents -- --nocapture`
//! `CLUAIZ_OFFICE_SAMPLES=<folder>` also prints what is extracted from every file in that folder.

mod support;

use anyhow::Result;
use cluaiz_core::media_limits::MediaLimits;
use cluaiz_media::documents::{self, Segment};
use cluaiz_media::formats::FormatRegistry;
use std::io::Cursor;

fn office(markdown: &str, format: office_oxide::DocumentFormat) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    office_oxide::create::create_from_markdown_to_writer(markdown, format, &mut out)?;
    Ok(out.into_inner())
}

fn text_of(bytes: &[u8], hint: &str) -> Result<String> {
    let format = FormatRegistry::default().detect(bytes, Some(hint))?;
    documents::extract(bytes, &format, MediaLimits::default())?.text(MediaLimits::default(), true)
}

#[test]
fn spreadsheet_rows_and_slides_are_read_in_order() -> Result<()> {
    let xlsx = office(
        "| Region | Q1 | Q2 |\n| --- | --- | --- |\n| North | 120 | 150 |\n| South | 80 | 95 |\n",
        office_oxide::DocumentFormat::Xlsx,
    )?;
    let text = text_of(&xlsx, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")?;
    assert!(text.contains("North") && text.contains("150") && text.contains("South"), "{text}");
    assert!(text.find("North") < text.find("South"));
    // Detected from the package without a hint
    let format = FormatRegistry::default().detect(&xlsx, None)?;
    assert!(documents::extract(&xlsx, &format, MediaLimits::default())?.text(MediaLimits::default(), true)?.contains("North"));

    let pptx = office("# Launch plan\n\nShip on 15 November\n\n---\n\n# Prices\n\nRs 499\n", office_oxide::DocumentFormat::Pptx)?;
    let text = text_of(&pptx, "application/vnd.openxmlformats-officedocument.presentationml.presentation")?;
    assert!(text.contains("15 November") && text.contains("Rs 499"), "{text}");
    assert!(text.find("15 November") < text.find("Rs 499"));

    // A small decoded budget is enforced
    let format = FormatRegistry::default().detect(&xlsx, None)?;
    assert!(documents::extract(&xlsx, &format, MediaLimits::new(1 << 20, 8)?).is_err());
    Ok(())
}

#[test]
fn word_pictures_keep_their_place_and_unread_drawings_are_marked() -> Result<()> {
    let png = support::png_bytes()?;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>Before the picture</w:t></w:r></w:p><w:p><w:r><w:drawing><a:graphic><a:graphicData><a:blip r:embed="rId9"/></a:graphicData></a:graphic></w:drawing></w:r></w:p><w:p><w:r><w:t>Between</w:t></w:r></w:p><w:p><w:r><w:drawing><a:graphic><a:graphicData/></a:graphic></w:drawing></w:r></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p></w:body></w:document>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let bytes = support::docx_with(xml, &[("word/_rels/document.xml.rels", rels.as_bytes()), ("word/media/image1.png", &png)])?;
    let format = FormatRegistry::default().detect(&bytes, Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"))?;

    // For a model that reads pictures: the picture's bytes in their place
    let segments = documents::segments(&bytes, &format, MediaLimits::default())?.expect("a Word document has segments");
    let kinds: Vec<&str> = segments.iter().map(|s| match s {
        Segment::Text(t) if t.trim().is_empty() => "",
        Segment::Text(_) => "text",
        Segment::Image(_) => "image",
        Segment::Unread(_) => "unread",
    }).filter(|k| !k.is_empty()).collect();
    assert_eq!(kinds, ["text", "image", "text", "unread", "text"]);
    assert!(segments.iter().any(|s| matches!(s, Segment::Image(d) if *d == png)));

    // For a model that reads text: labels in place; the document is complete, not refused
    let document = documents::extract_with_assets(&bytes, &format, MediaLimits::default(), None)?;
    assert!(document.complete());
    let text = document.text(MediaLimits::default(), true)?;
    assert!(text.find("Before") < text.find("[Picture 1") && text.find("[Picture 1") < text.find("Between"), "{text}");
    assert!(text.contains("[Picture 2: a drawing without a picture (a chart or a shape); its content is not in this text]"), "{text}");
    assert!(text.find("[Picture 2") < text.find("After"));
    Ok(())
}

#[test]
fn print_samples_when_given() -> Result<()> {
    let Ok(folder) = std::env::var("CLUAIZ_OFFICE_SAMPLES") else { return Ok(()) };
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else { continue };
        if !["xlsx", "pptx", "docx", "xls", "ppt", "doc"].contains(&extension) {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let hint = mime_guess::from_ext(extension).first_raw();
        let format = FormatRegistry::default().detect(&bytes, hint)?;
        let started = std::time::Instant::now();
        let document = documents::extract(&bytes, &format, MediaLimits::default())?;
        let elapsed = started.elapsed();
        let pictures = documents::segments(&bytes, &format, MediaLimits::default())?
            .map_or(0, |s| s.iter().filter(|s| matches!(s, Segment::Image(_))).count());
        println!("===== {} ({:?}, {} pictures, complete {})", path.display(), elapsed, pictures, document.complete());
        println!("{}", document.text(MediaLimits::default(), false)?);
    }
    Ok(())
}
