//! Shared image decoding. Model resize/normalization belongs to the selected backend.

use ::image::io::{Limits, Reader};
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits};
use std::io::Cursor;

// Bound a largest decoded RGBA F32 pixel plus its simultaneously retained RGB8 output.
const PIXEL_WORKSPACE_BYTES: u64 =
    (std::mem::size_of::<[f32; 4]>() + std::mem::size_of::<[u8; 3]>()) as u64;

/// Decodes encoded image bytes into packed RGB8, enforcing size checks before decoding.
pub fn decode_image(bytes: &[u8], limits: MediaLimits) -> Result<DecodedImage> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    let (width, height) = Reader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    if width == 0 || height == 0 {
        bail!("Image dimensions must be non-zero");
    }
    let workspace = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(PIXEL_WORKSPACE_BYTES))
        .context("Image workspace overflow")?;
    limits.check_decoded_bytes(workspace)?;
    let mut reader = Reader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder_limits = Limits::default();
    decoder_limits.max_alloc = Some(limits.max_decoded_bytes());
    reader.limits(decoder_limits);
    let image = reader.decode()?.to_rgb8();
    Ok(DecodedImage::new(width, height, image.into_raw().into())?)
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
    fn test_advertised_formats_follow_the_actual_byte_decoder() {
        let limits = MediaLimits::new(cluaiz_core::MIB, cluaiz_core::MIB).unwrap();
        let tga = fixture(ImageOutputFormat::Tga);
        assert!(decode_image(&tga, limits).is_err());
        let descriptors = crate::formats::attachment_formats();
        assert!(!descriptors.iter().any(|(_, extension)| *extension == "tga"));
        let qoi = fixture(ImageOutputFormat::Qoi);
        assert!(decode_image(&qoi, limits).is_ok());
        let canonical = crate::formats::canonical_extensions(
            descriptors.iter().map(|(_, extension)| *extension),
        );
        assert!(canonical.contains(&"qoi"));
        for format in ::image::ImageFormat::all().filter(|format| !format.reading_enabled()) {
            assert!(!descriptors
                .iter()
                .any(|(_, extension)| format.extensions_str().contains(extension)));
        }
    }
}
