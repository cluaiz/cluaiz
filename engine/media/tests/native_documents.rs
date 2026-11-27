//! Real PDFium rendering with the pinned build that the Engine downloads on first use, and the
//! native media manifest checks. No inference model dependency.
#[allow(dead_code)]
mod support;
use anyhow::Result;
use cluaiz_core::media_limits::MediaLimits;
use cluaiz_media::{
    documents::{self, pdf_render},
    formats::FormatRegistry,
    native_assets::{NativeAssets, MANIFEST_NAME},
};

/// Downloads PDFium (about 4 MB) into `<CLUAIZ_HOME>/bin` of the test's environment when it is
/// not there yet. Set CLUAIZ_HOME to a temporary folder.
#[test]
#[ignore = "downloads the pinned PDFium build from GitHub on first run"]
fn real_pdfium_renders_and_only_pages_without_text_need_it() -> Result<()> {
    let limits = MediaLimits::default();
    let text_pdf = support::pdf(&[Some("Scanned order code 741923")])?;
    let image = pdf_render::with_document(&text_pdf, |document| {
        assert_eq!(document.page_count()?, 1);
        assert!(document.render_page(0, limits).is_err());
        assert!(document.render_page(2, limits).is_err());
        assert!(document
            .render_page(1, MediaLimits::new(1024 * 1024, 1)?)
            .is_err());
        document.render_page(1, limits)
    })?;
    // US Letter at 150 DPI
    assert_eq!((image.width(), image.height()), (1275, 1650));
    let mut colors = pdf_extract::Document::load_mem(&support::pdf(&[None])?)?;
    colors
        .get_object_mut((5, 0))?
        .as_stream_mut()?
        .set_content(b"1 0 0 rg 0 0 306 792 re f 0 0 1 rg 306 0 306 792 re f".to_vec());
    let mut color_pdf = Vec::new();
    colors.save_to(&mut color_pdf)?;
    let color_image = pdf_render::with_document(&color_pdf, |document| document.render_page(1, limits))?;
    let left = (825 * 1275 + 100) * 3;
    let right = (825 * 1275 + 1100) * 3;
    assert_eq!(&color_image.pixels()[left..left + 3], &[255, 0, 0]);
    assert_eq!(&color_image.pixels()[right..right + 3], &[0, 0, 255]);
    println!("REAL PDFIUM RGB: 1275x1650 page, exact red/blue pixels and row order passed");
    // A text page followed by a scanned page: only page 2 is left for the vision model
    let mixed = support::raster::pdf(&image, Some("Embedded first page 826417"))?;
    assert!(!mixed.windows(6).any(|window| window == b"741923"));
    let format = FormatRegistry::default().detect(&mixed, None)?;
    let document = documents::extract(&mixed, &format, limits)?;
    assert_eq!(documents::pages_without_text(&document.parts), [2]);
    assert!(document.parts[0].text.contains("826417"));
    let rendered = pdf_render::with_document(&mixed, |document| document.render_page(2, limits))?;
    assert_eq!((rendered.width(), rendered.height()), (image.width(), image.height()));
    assert!(pdf_render::with_document(b"invalid PDF", |_| Ok(())).is_err());
    documents::release_native_providers()?;
    pdf_render::with_document(&text_pdf, |document| document.page_count())?;
    documents::release_native_providers()?;
    println!("REAL PDFIUM SPLIT: text page read from its layer, scanned page 2 rendered; corrupt PDF refused; unload and reload passed");
    Ok(())
}

#[test]
fn test_native_manifest_rejects_platform_traversal_hash_and_size() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut manifest = serde_json::json!({"schema_version":1,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "files":{"library":{"path":"../outside.dll","sha256":"00".repeat(32),"source":"negative fixture"}}});
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
