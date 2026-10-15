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
