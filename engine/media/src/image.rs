//! Shared image decoding. Model resize/normalization belongs to the selected backend.

use ::image::io::{Limits, Reader};
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits};
use std::io::Cursor;

/// Decodes into packed RGB8 with bounded decoding and checked conversion/copy peaks.
pub fn decode_image(bytes: &[u8], limits: MediaLimits) -> Result<DecodedImage> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    if is_svg(bytes) {
        // Vector drawings have no pixels to read; llama.cpp's image loader does not read them either
        bail!(
            "SVG images are not supported: export the drawing as PNG or JPEG and attach it again"
        );
    }
    let mut reader = Reader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder_limits = Limits::default();
    decoder_limits.max_alloc = Some(limits.max_decoded_bytes());
    reader.limits(decoder_limits);
    // Reader checks the decoder's actual output size before allocating its pixels.
    let decoded = reader.decode()?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        bail!("Image dimensions must be non-zero");
    }
    let rgb_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(std::mem::size_of::<[u8; 3]>() as u64))
        .context("Image workspace overflow")?;
    let conversion_peak = u64::try_from(decoded.as_bytes().len())?
        .checked_add(rgb_bytes)
        .context("Image conversion workspace overflow")?;
    // Vec -> Arc<[u8]> copies while the original RGB allocation is still alive.
    let ownership_peak = rgb_bytes
        .checked_mul(2)
        .context("Image ownership workspace overflow")?;
    let workspace = conversion_peak.max(ownership_peak);
    limits.check_decoded_bytes(workspace)?;
    let image = decoded.into_rgb8();
    Ok(DecodedImage::new(width, height, image.into_raw().into())?)
}

/// An SVG document: an `<svg` element near the start (after any BOM, XML declaration or comments).
fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
        && text.contains("<svg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, ImageOutputFormat, RgbImage};

    fn fixture(format: ImageOutputFormat) -> Vec<u8> {
        let image = RgbImage::from_raw(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image)
            .write_to(&mut encoded, format)
            .unwrap();
        encoded.into_inner()
    }

    #[test]
    fn test_image_png_exact_pixels_and_jpeg_dimensions_decode() {
        let png = fixture(ImageOutputFormat::Png);
        let limits = MediaLimits::new(png.len() as u64, cluaiz_core::MIB).unwrap();
        let decoded = decode_image(&png, limits).unwrap();
        assert_eq!(decoded.pixels(), &[255, 0, 0, 0, 255, 0]);
        let jpeg = fixture(ImageOutputFormat::Jpeg(90));
        let limits = MediaLimits::new(jpeg.len() as u64, cluaiz_core::MIB).unwrap();
        let decoded = decode_image(&jpeg, limits).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 1));
    }

    #[test]
    fn test_image_corrupt_input_and_workspace_limit_reject() {
        let bytes = fixture(ImageOutputFormat::Png);
        assert!(decode_image(&bytes, MediaLimits::new(bytes.len() as u64, 1).unwrap()).is_err());
        assert!(decode_image(&bytes, MediaLimits::new(1, cluaiz_core::MIB).unwrap()).is_err());
        assert!(decode_image(b"invalid image", MediaLimits::new(100, 100).unwrap()).is_err());
    }

    #[test]
    fn test_svg_is_rejected_with_a_clear_message() {
        let svg = b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<!-- diagram -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>";
        let error = decode_image(svg, MediaLimits::new(1024, 1024).unwrap()).unwrap_err();
        assert!(error.to_string().contains("SVG images are not supported"));
        let png = fixture(ImageOutputFormat::Png);
        assert!(!is_svg(&png));
    }

    #[test]
    fn test_actual_color_depth_and_shared_buffer_copy_are_bounded() -> Result<()> {
        use ::image::{ImageBuffer, RgbaImage};
        let dimensions = (64, 64);
        let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
        for (image, bytes_per_pixel) in [
            (
                DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, ::image::Rgb([12, 34, 56]))),
                6,
            ),
            (
                DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                    64,
                    64,
                    ::image::Rgba([12, 34, 56, 78]),
                )),
                7,
            ),
            (
                DynamicImage::ImageRgba16(ImageBuffer::from_pixel(
                    64,
                    64,
                    ::image::Rgba([1234, 5678, 9012, 3456]),
                )),
                11,
            ),
        ] {
            let reference = image.to_rgb8();
            let mut encoded = Cursor::new(Vec::new());
            image.write_to(&mut encoded, ImageOutputFormat::Png)?;
            let bytes = encoded.into_inner();
            let peak = pixels * bytes_per_pixel;
            let rejected = decode_image(&bytes, MediaLimits::new(bytes.len() as u64, peak - 1)?);
            assert!(rejected
                .unwrap_err()
                .to_string()
                .contains("DecodedData requires"));
            let decoded = decode_image(&bytes, MediaLimits::new(bytes.len() as u64, peak)?)?;
            assert_eq!((decoded.width(), decoded.height()), dimensions);
            assert!(decoded.pixels() == reference.as_raw(), "RGB pixels changed");
        }
        Ok(())
    }

    #[test]
    fn test_advertised_formats_follow_the_actual_byte_decoder() {
        let limits = MediaLimits::new(cluaiz_core::MIB, cluaiz_core::MIB).unwrap();
        let tga = fixture(ImageOutputFormat::Tga);
        assert!(decode_image(&tga, limits).is_err());
        let descriptors = crate::formats::attachment_formats();
        assert!(!descriptors.iter().any(|(_, extension)| *extension == "tga"));
        let qoi = fixture(ImageOutputFormat::Qoi);
        assert!(decode_image(&qoi, limits).is_ok());
        let extensions =
            crate::formats::unique_extensions(descriptors.iter().map(|(_, extension)| *extension));
        assert!(extensions.contains(&"qoi"));
        for format in ::image::ImageFormat::all().filter(|format| !format.reading_enabled()) {
            assert!(!descriptors
                .iter()
                .any(|(_, extension)| format.extensions_str().contains(extension)));
        }
    }

    #[test]
    #[ignore = "requires CLUAIZ_MEDIA_TEST_IMAGE pointing to a real local image"]
    fn test_actual_large_image_retains_pixels_with_original_request_ceiling() -> Result<()> {
        let bytes = std::fs::read(std::env::var("CLUAIZ_MEDIA_TEST_IMAGE")?)?;
        let limit = std::env::var("CLUAIZ_MEDIA_TEST_DECODED_LIMIT")?.parse::<u64>()?;
        let limits = MediaLimits::new(
            cluaiz_core::media_limits::DEFAULT_MEDIA_INPUT_BYTES.get(),
            limit,
        )?;
        let reference = ::image::load_from_memory(&bytes)?.into_rgb8();
        for request in 0..3 {
            let image = decode_image(&bytes, limits)?;
            assert_eq!((image.width(), image.height()), reference.dimensions());
            assert!(image.pixels() == reference.as_raw(), "RGB pixels changed");
            println!("ACTUAL IMAGE DECODE: request={request}; dimensions={}x{}; rgb_bytes={}; ceiling={limit}; exact_pixels=true", image.width(), image.height(), image.pixels().len());
        }
        Ok(())
    }
}
