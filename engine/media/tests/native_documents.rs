//! Opt-in real packaged native provider acceptance, no inference model dependency.
#[allow(dead_code)]
mod support;
use anyhow::{Context, Result};
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits};
use cluaiz_media::{
    documents::{self, pdf_render},
    formats::FormatRegistry,
    native_assets::{NativeAssets, MANIFEST_NAME},
};

#[test]
#[ignore = "requires CLUAIZ_NATIVE_MEDIA_TEST_PACKAGE with real PDFium/Tesseract/language assets"]
fn real_native_ocr_scanned_mixed_pdf_images_and_reuse() -> Result<()> {
    let package = std::env::var("CLUAIZ_NATIVE_MEDIA_TEST_PACKAGE")?;
    let assets = NativeAssets::read(std::path::Path::new(&package))?;
    assets.verify()?;
    let limits = MediaLimits::default();
    let text_pdf = support::pdf(&[Some("Scanned order code 741923")])?;
    let image = pdf_render::with_document(&text_pdf, &assets, |document| {
        assert_eq!(document.page_count()?, 1);
        assert!(document.render_page(0, limits).is_err());
        assert!(document.render_page(2, limits).is_err());
        assert!(document
            .render_page(1, MediaLimits::new(1024 * 1024, 1)?)
            .is_err());
        document.render_page(1, limits)
    })?;
    assert_eq!((image.width(), image.height()), (1275, 1650));
    let mut colors = pdf_extract::Document::load_mem(&support::pdf(&[None])?)?;
    colors
        .get_object_mut((5, 0))?
        .as_stream_mut()?
        .set_content(b"1 0 0 rg 0 0 306 792 re f 0 0 1 rg 306 0 306 792 re f".to_vec());
    let mut color_pdf = Vec::new();
    colors.save_to(&mut color_pdf)?;
    let color_image = pdf_render::with_document(&color_pdf, &assets, |document| {
        document.render_page(1, limits)
    })?;
    let left = (825 * 1275 + 100) * 3;
    let right = (825 * 1275 + 1100) * 3;
    assert_eq!(&color_image.pixels()[left..left + 3], &[255, 0, 0]);
    assert_eq!(&color_image.pixels()[right..right + 3], &[0, 0, 255]);
    println!("REAL NATIVE PDF RGB: odd width 1275, exact red/blue pixels and contiguous row order passed");
    for format in [
        image::ImageOutputFormat::Png,
        image::ImageOutputFormat::Jpeg(95),
    ] {
        let bytes = support::raster::encoded(&image, format)?;
        let format = FormatRegistry::default().detect(&bytes, None)?;
        let document = documents::extract_with_assets(&bytes, &format, limits, Some(&assets))?;
        assert!(document.complete(), "{:?}", document.parts);
        assert!(document.text(limits, true)?.contains("741923"));
        assert!(document.parts[0].ocr_confidence.is_some());
        println!(
            "REAL NATIVE OCR: {} -> code 741923; confidence {:?}",
            format.mime_type, document.parts[0].ocr_confidence
        );
    }
    for first_text in [None, Some("Embedded first page 826417")] {
        let bytes = support::raster::pdf(&image, first_text)?;
        // Recognition cannot cheat by reading the original digit string from PDF.
        assert!(!bytes.windows(6).any(|window| window == b"741923"));
        let format = FormatRegistry::default().detect(&bytes, None)?;
        let document = documents::extract_with_assets(&bytes, &format, limits, Some(&assets))?;
        assert!(document.complete(), "{:?}", document.parts);
        let text = document.text(limits, true)?;
        assert!(text.contains("741923"), "{text}");
        let scan = document.parts.last().context("Scanned part")?;
        assert_eq!(scan.method, "pdf_ocr");
        assert_eq!(scan.page, Some(if first_text.is_some() { 2 } else { 1 }));
        if first_text.is_some() {
            assert!(text.find("826417") < text.find("741923"));
        }
        println!("REAL NATIVE PDF: raster-only/mixed page ordering -> code 741923; no GGUF or ONNX session");
    }
    // Same-page visual/text content and whitespace-only layers exercise both
    // merge and shorter-replacement accounting; no text may silently be lost.
    for overlay in ["Embedded layer 826417".to_owned(), " ".repeat(400)] {
        let font_source = pdf_extract::Document::load_mem(&support::pdf(&[Some("font")])?)?;
        let fonts = font_source
            .get_dictionary((4, 0))?
            .get(b"Resources")?
            .as_dict()?
            .get(b"Font")?
            .clone();
        let mut layered = pdf_extract::Document::load_mem(&support::raster::pdf(&image, None)?)?;
        layered
            .get_object_mut((4, 0))?
            .as_dict_mut()?
            .get_mut(b"Resources")?
            .as_dict_mut()?
            .set("Font", fonts);
        let mut content = layered.get_object((5, 0))?.as_stream()?.content.clone();
        content.extend_from_slice(format!("\nBT /F1 12 Tf 72 620 Td ({overlay}) Tj ET").as_bytes());
        layered
            .get_object_mut((5, 0))?
            .as_stream_mut()?
            .set_content(content);
        let mut bytes = Vec::new();
        layered.save_to(&mut bytes)?;
        let embedded = pdf_extract::extract_text_from_mem(&bytes)?;
        if overlay.trim().is_empty() {
            assert!(embedded.trim().is_empty() && embedded.len() > 100);
        }
        let format = FormatRegistry::default().detect(&bytes, None)?;
        let document = documents::extract_with_assets(&bytes, &format, limits, Some(&assets))?;
        assert!(document.complete(), "{:?}", document.parts);
        let text = document.text(limits, true)?;
        assert!(text.contains("741923"), "{text}");
        if !overlay.trim().is_empty() {
            assert_eq!(document.parts[0].method, "pdf_text_and_ocr");
            assert!(text.contains("826417"));
        } else {
            assert_eq!(document.parts[0].method, "pdf_ocr");
        }
    }
    println!("REAL NATIVE LAYERED PDF: embedded text preserved; whitespace layer replaced without byte-accounting underflow");
    let other = support::pdf(&[Some("Different receipt code 539281")])?;
    let other =
        pdf_render::with_document(&other, &assets, |document| document.render_page(1, limits))?;
    let recognized = documents::ocr::recognize(&other, limits, &assets)?;
    assert!(recognized.text.contains("539281") && !recognized.text.contains("741923"));
    let white = DecodedImage::new(1275, 1650, vec![255; 1275 * 1650 * 3].into())?;
    assert!(documents::ocr::recognize(&white, limits, &assets).is_err());
    assert!(documents::ocr::recognize(&image, limits, &assets)?
        .text
        .contains("741923"));
    assert!(pdf_render::with_document(b"invalid PDF", &assets, |_| Ok(())).is_err());
    println!("REAL NATIVE REUSE: changed image, empty recognition failure, following request, corrupt PDF and pre-allocation bitmap bounds passed");
    std::thread::scope(|scope| -> Result<()> {
        let mut workers = Vec::new();
        for _ in 0..4 {
            let image = &image;
            let assets = &assets;
            workers.push(scope.spawn(move || documents::ocr::recognize(image, limits, assets)));
        }
        for worker in workers {
            assert!(worker
                .join()
                .map_err(|_| anyhow::anyhow!("OCR worker panicked"))??
                .text
                .contains("741923"));
        }
        Ok(())
    })?;
    for run in 1..=3 {
        let start = std::time::Instant::now();
        assert!(documents::ocr::recognize(&image, limits, &assets)?
            .text
            .contains("741923"));
        println!(
            "REAL NATIVE OCR MEASURE: warm CPU run {run}: {:?}",
            start.elapsed()
        );
    }
    if let Ok(path) = std::env::var("CLUAIZ_OCR_HINDI_IMAGE") {
        let bytes = std::fs::read(path)?;
        let image = cluaiz_media::image::decode_image(&bytes, limits)?;
        let result = documents::ocr::recognize(&image, limits, &assets)?;
        assert!(result.text.contains("नमस्ते"), "{}", result.text);
        println!("REAL NATIVE OCR HINDI: {}", result.text.trim());
    }
    documents::release_native_providers()?;
    assert!(documents::ocr::recognize(&image, limits, &assets)?
        .text
        .contains("741923"));
    documents::release_native_providers()?;
    println!("REAL NATIVE LIFETIME: concurrent recognition, explicit unload, reload and second unload passed");
    Ok(())
}

#[test]
fn test_native_manifest_rejects_platform_traversal_hash_and_size() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut manifest = serde_json::json!({"schema_version":1,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "files":{"library":{"path":"../outside.dll","sha256":"00".repeat(32),"source":"negative fixture"}},"ocr":null,"pdf":null});
    std::fs::write(directory.path().join(MANIFEST_NAME), manifest.to_string())?;
    assert!(NativeAssets::read(directory.path()).is_err());
    manifest["files"]["library"]["path"] = "damaged.dll".into();
    std::fs::write(
        directory.path().join("damaged.dll"),
        b"damaged native asset",
    )?;
    std::fs::write(directory.path().join(MANIFEST_NAME), manifest.to_string())?;
    assert!(NativeAssets::read(directory.path())?.verify().is_err());
    let initial = NativeAssets::read(directory.path())?.identity;
    std::fs::write(
        directory.path().join("damaged.dll"),
        b"changed native asset bytes",
    )?;
    assert_ne!(NativeAssets::read(directory.path())?.identity, initial);
    std::fs::remove_file(directory.path().join("damaged.dll"))?;
    assert!(NativeAssets::read(directory.path()).is_err());
    manifest["os"] = "other-platform".into();
    std::fs::write(directory.path().join(MANIFEST_NAME), manifest.to_string())?;
    assert!(NativeAssets::read(directory.path()).is_err());
    std::fs::write(directory.path().join(MANIFEST_NAME), vec![b' '; 65537])?;
    assert!(NativeAssets::read(directory.path()).is_err());
    Ok(())
}
