//! Real raster-only PDF pages; recognition cannot read an embedded text layer.
use cluaiz_core::media::DecodedImage;
use std::io::{Cursor, Write};

pub fn encoded(image: &DecodedImage, format: image::ImageOutputFormat) -> anyhow::Result<Vec<u8>> {
    let image = image::RgbImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
        .ok_or_else(|| anyhow::anyhow!("Invalid fixture image"))?;
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut output, format)?;
    Ok(output.into_inner())
}

pub fn pdf(image: &DecodedImage, first_text: Option<&str>) -> anyhow::Result<Vec<u8>> {
    let jpeg = encoded(image, image::ImageOutputFormat::Jpeg(95))?;
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            if first_text.is_some() { 2 } else { 1 },
            if first_text.is_some() {
                "4 0 R 7 0 R"
            } else {
                "4 0 R"
            }
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    objects.push(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Scan 6 0 R >> >> /Contents 5 0 R >>".to_vec());
    let content = b"q 612 0 0 792 0 0 cm /Scan Do Q";
    objects.push(
        format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            std::str::from_utf8(content)?
        )
        .into_bytes(),
    );
    let mut scan = format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", image.width(), image.height(), jpeg.len()).into_bytes();
    scan.extend_from_slice(&jpeg);
    scan.extend_from_slice(b"\nendstream");
    objects.push(scan);
    if let Some(text) = first_text {
        // The text page comes before the scan in the actual page tree.
        objects[1] = b"<< /Type /Pages /Count 2 /Kids [7 0 R 4 0 R] >>".to_vec();
        objects.push(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 8 0 R >>".to_vec());
        let content = format!(
            "BT /F1 12 Tf 72 720 Td ({}) Tj ET",
            text.replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)")
        );
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        );
    }
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(bytes, "{} 0 obj", index + 1)?;
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    writeln!(bytes, "xref\n0 {}\n0000000000 65535 f ", objects.len() + 1)?;
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ")?;
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
        objects.len() + 1
    )?;
    Ok(bytes)
}
