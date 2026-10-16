//! Genuine small document builders shared by extraction and HTTP acceptance tests only.
use std::io::{Cursor, Write};
// Opt-in native/API fixtures use raster helpers; the regular document test does not.
#[allow(dead_code)]
pub mod raster;

pub fn pdf(pages: &[Option<&str>]) -> anyhow::Result<Vec<u8>> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages.len(),
            (0..pages.len())
                .map(|i| format!("{} 0 R", 4 + i * 2))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_owned(),
    ];
    for (index, page) in pages.iter().enumerate() {
        let content = match page {
            Some(text) => format!(
                "BT /F1 12 Tf 72 720 Td ({}) Tj ET",
                text.replace('\\', "\\\\")
                    .replace('(', "\\(")
                    .replace(')', "\\)")
            ),
            None => "q Q".to_owned(),
        };
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + index * 2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            content
        ));
    }
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(bytes, "{} 0 obj\n{}\nendobj", index + 1, object)?;
    }
    let xref = bytes.len();
    writeln!(bytes, "xref\n0 {}\n0000000000 65535 f ", objects.len() + 1)?;
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ")?;
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref
    )?;
    Ok(bytes)
}

pub fn docx(xml: &str) -> anyhow::Result<Vec<u8>> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (path, text) in [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/document.xml", xml),
    ] {
        archive.start_file(path, options)?;
        archive.write_all(text.as_bytes())?;
    }
    Ok(archive.finish()?.into_inner())
}

pub const DOCX_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><a:document xmlns:a="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><a:body><a:p><a:r><a:t>Hello &amp; नमस्ते</a:t></a:r></a:p><a:tbl><a:tr><a:tc><a:p><a:r><a:t>First</a:t></a:r></a:p></a:tc><a:tc><a:p><a:r><a:t>Second</a:t></a:r></a:p></a:tc></a:tr></a:tbl><a:p><a:r><a:t>Final</a:t></a:r></a:p></a:body></a:document>"#;

/// Word 97 CFB/FIB/CLX document with one actual ANSI or UTF-16 text piece.
#[allow(dead_code)] // Other consumers use only the PDF/DOCX builders in this owner.
pub fn doc(text: &str, compressed: bool, table1: bool) -> anyhow::Result<Vec<u8>> {
    let mut word = vec![0u8; 0x400];
    word[0..2].copy_from_slice(&0xa5ecu16.to_le_bytes());
    word[2..4].copy_from_slice(&0x00c1u16.to_le_bytes());
    word[6..8].copy_from_slice(&0x0409u16.to_le_bytes());
    let flags = 4u16 | if table1 { 1 << 9 } else { 0 };
    word[0x0a..0x0c].copy_from_slice(&flags.to_le_bytes());
    word[0x20..0x22].copy_from_slice(&14u16.to_le_bytes());
    word[0x3e..0x40].copy_from_slice(&22u16.to_le_bytes());
    word[0x98..0x9a].copy_from_slice(&0x005du16.to_le_bytes());
    let count = u32::try_from(text.encode_utf16().count())?;
    word[0x4c..0x50].copy_from_slice(&count.to_le_bytes());
    word[0x18..0x1c].copy_from_slice(&0x400u32.to_le_bytes());
    if compressed {
        anyhow::ensure!(text.is_ascii(), "ANSI fixture takes ASCII input");
        word.extend_from_slice(text.as_bytes());
    } else {
        word.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    }
    let end = u32::try_from(word.len())?;
    word[0x1c..0x20].copy_from_slice(&end.to_le_bytes());
    word[0x40..0x44].copy_from_slice(&end.to_le_bytes());
    let mut table = vec![2u8];
    table.extend_from_slice(&16u32.to_le_bytes());
    table.extend_from_slice(&0u32.to_le_bytes());
    table.extend_from_slice(&count.to_le_bytes());
    table.extend_from_slice(&0u16.to_le_bytes());
    let fc = if compressed { 0x4000_0800u32 } else { 0x400u32 };
    table.extend_from_slice(&fc.to_le_bytes());
    table.extend_from_slice(&0u16.to_le_bytes());
    word[0x1a6..0x1aa].copy_from_slice(&u32::try_from(table.len())?.to_le_bytes());
    let mut container = cfb::CompoundFile::create(Cursor::new(Vec::new()))?;
    container.create_stream("/WordDocument")?.write_all(&word)?;
    container
        .create_stream(if table1 { "/1Table" } else { "/0Table" })?
        .write_all(&table)?;
    Ok(container.into_inner().into_inner())
}
