//! Package-configured RGB preparation. Decoding belongs to cluaiz-media.
use crate::{text::EmbeddingInput, OnnxEngine};
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits, EngineError};
use image::imageops::FilterType;
use ort::value::{TensorElementType, ValueType};
use serde_json::Value;

#[derive(Debug)]
pub(crate) struct VisionProcessor {
    config: Value,
}
impl VisionProcessor {
    pub(crate) fn load(dir: &std::path::Path) -> Result<Self> {
        let mut config: Value = serde_json::from_slice(
            &std::fs::read(dir.join("preprocessor_config.json"))
                .context("Image embedding requires preprocessor_config.json")?,
        )?;
        // Transformers v4.11 CLIPFeatureExtractor converts integer RGB to
        // float/255 inside normalize; its serialized schema has no rescale fields.
        // Only this explicit legacy schema gets that conversion, never arbitrary models.
        if config.get("feature_extractor_type").and_then(Value::as_str)
            == Some("CLIPFeatureExtractor")
            && config.get("do_rescale").is_none()
            && config.get("do_normalize").and_then(Value::as_bool) == Some(true)
        {
            config["do_rescale"] = Value::Bool(true);
            config["rescale_factor"] = serde_json::json!(1.0 / f64::from(u8::MAX));
        }
        Ok(Self { config })
    }
    fn flag(&self, name: &str) -> Result<bool> {
        self.config
            .get(name)
            .and_then(Value::as_bool)
            .with_context(|| format!("Image processor requires boolean {name}"))
    }
    pub(crate) fn prepare(
        &self,
        image: &DecodedImage,
        dtype: &ValueType,
        limits: MediaLimits,
    ) -> Result<([usize; 4], Vec<f32>)> {
        let ValueType::Tensor {
            ty,
            shape,
            dimension_symbols,
        } = dtype
        else {
            bail!("Image input is not a tensor")
        };
        if *ty != TensorElementType::Float32 || shape.len() != 4 || !matches!(shape[0], -1 | 1) {
            bail!("Image embedding currently requires a batch-one rank-four F32 graph");
        }
        let channel_axis = |axis: usize| {
            shape[axis] == 3
                || (shape[axis] == -1
                    && matches!(
                        dimension_symbols[axis].as_str(),
                        "num_channels" | "channels"
                    ))
        };
        let chw = match (channel_axis(1), channel_axis(3)) {
            (true, false) => true,
            (false, true) => false,
            _ => bail!("Image graph must identify one RGB channel axis through shape or dimension symbols (NCHW or NHWC)"),
        };
        limits.check_decoded_bytes(image.pixels().len() as u64)?;
        let mut rgb =
            image::RgbImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
                .context("Invalid shared RGB image")?;
        if self.flag("do_resize")? {
            let size = self
                .config
                .get("size")
                .context("Image resize size missing")?;
            let (width, height) = if let Some(shortest) = size
                .as_u64()
                .or_else(|| size.get("shortest_edge").and_then(Value::as_u64))
            {
                let shortest = u32::try_from(shortest)?;
                if shortest == 0 {
                    bail!("Image resize dimension is zero");
                }
                let edge = u64::from(rgb.width().min(rgb.height()));
                (
                    u32::try_from(u64::from(rgb.width()) * u64::from(shortest) / edge)?,
                    u32::try_from(u64::from(rgb.height()) * u64::from(shortest) / edge)?,
                )
            } else {
                dimensions(size)?
            };
            check_pixels(width, height, limits)?;
            let filter = match self.config.get("resample").and_then(Value::as_u64) {
                Some(0) => FilterType::Nearest,
                Some(1) => FilterType::Lanczos3,
                Some(2) => FilterType::Triangle,
                Some(3) => FilterType::CatmullRom,
                _ => bail!("Unsupported or missing image resample contract"),
            };
            rgb = image::imageops::resize(&rgb, width, height, filter);
        }
        if self.flag("do_center_crop")? {
            let (width, height) =
                dimensions(self.config.get("crop_size").context("Crop size missing")?)?;
            check_pixels(width, height, limits)?;
            if width > rgb.width() || height > rgb.height() {
                bail!("Image crop padding is not supported by this processor contract");
            }
            let left = (rgb.width() - width) / 2;
            let top = (rgb.height() - height) / 2;
            rgb = image::imageops::crop_imm(&rgb, left, top, width, height).to_image();
        }
        let (width, height) = (rgb.width() as usize, rgb.height() as usize);
        let expected = if chw {
            [1, 3, height, width]
        } else {
            [1, height, width, 3]
        };
        crate::media::validate_tensor(
            dtype,
            TensorElementType::Float32,
            &expected.map(|v| v as i64),
        )?;
        let factor = if self.flag("do_rescale")? {
            let factor = self
                .config
                .get("rescale_factor")
                .and_then(Value::as_f64)
                .context("Image rescale factor missing")? as f32;
            if !factor.is_finite() || factor <= 0.0 {
                bail!("Invalid image rescale factor");
            }
            factor
        } else {
            1.0
        };
        let (mean, std) = if self.flag("do_normalize")? {
            let mean = channels(&self.config, "image_mean")?;
            let std = channels(&self.config, "image_std")?;
            if std.iter().any(|v| *v <= 0.0) {
                bail!("Image standard deviation must be positive");
            }
            (mean, std)
        } else {
            ([0.0; 3], [1.0; 3])
        };
        let count = width
            .checked_mul(height)
            .and_then(|v| v.checked_mul(3))
            .context("Image tensor size overflow")?;
        limits.check_decoded_bytes(
            u64::try_from(count)?
                .checked_mul(4)
                .context("Image tensor bytes overflow")?,
        )?;
        let mut data = vec![0.0; count];
        for (index, pixel) in rgb.pixels().enumerate() {
            for channel in 0..3 {
                let target = if chw {
                    channel * width * height + index
                } else {
                    index * 3 + channel
                };
                data[target] = (f32::from(pixel[channel]) * factor - mean[channel]) / std[channel];
            }
        }
        if data.iter().any(|v| !v.is_finite()) {
            bail!("Image normalization produced non-finite values");
        }
        Ok((expected, data))
    }
}
fn dimensions(value: &Value) -> Result<(u32, u32)> {
    let pair = if let Some(side) = value.as_u64() {
        (side, side)
    } else {
        (
            value
                .get("width")
                .and_then(Value::as_u64)
                .context("Image width missing")?,
            value
                .get("height")
                .and_then(Value::as_u64)
                .context("Image height missing")?,
        )
    };
    let pair = (u32::try_from(pair.0)?, u32::try_from(pair.1)?);
    if pair.0 == 0 || pair.1 == 0 {
        bail!("Image dimensions must be positive");
    }
    Ok(pair)
}
fn check_pixels(width: u32, height: u32, limits: MediaLimits) -> Result<()> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|v| v.checked_mul(3))
        .context("Image allocation overflow")?;
    limits.check_decoded_bytes(bytes)?;
    Ok(())
}
fn channels(config: &Value, name: &str) -> Result<[f32; 3]> {
    let values = config
        .get(name)
        .and_then(Value::as_array)
        .context("Image normalization channels missing")?;
    if values.len() != 3 {
        bail!("{name} must contain three RGB values");
    }
    let mut result = [0.0; 3];
    for (out, value) in result.iter_mut().zip(values) {
        *out = value.as_f64().context("Invalid normalization number")? as f32;
        if !out.is_finite() {
            bail!("Non-finite image normalization parameter");
        }
    }
    Ok(result)
}
impl OnnxEngine {
    /// Compatibility byte entry: decode once, then use the typed graph executor.
    pub fn execute_vision_embedding(&self, bytes: &[u8]) -> Result<Vec<f32>, EngineError> {
        self.execute_image_embedding(bytes, None)
    }
    pub(crate) fn execute_image_embedding(
        &self,
        bytes: &[u8],
        instruction: Option<String>,
    ) -> Result<Vec<f32>, EngineError> {
        let limits = cluaiz_core::Settings::current().media.limits;
        let image = cluaiz_media::image::decode_image(bytes, limits)
            .map_err(|e| EngineError::EmbeddingFailed(e.to_string()))?;
        self.execute_embedding(
            &EmbeddingInput::Image {
                image,
                text: instruction.unwrap_or_default(),
            },
            limits,
        )
        .map(|result| result.vector)
        .map_err(|e| EngineError::EmbeddingFailed(format!("{e:#}")))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn processor() -> VisionProcessor {
        VisionProcessor {
            config: serde_json::json!({
        "do_resize":false,"do_center_crop":false,"do_rescale":true,"rescale_factor":0.5,
        "do_normalize":true,"image_mean":[1,2,3],"image_std":[1,2,4]}),
        }
    }
    #[test]
    fn test_processor_respects_layout_values_and_rejects_invalid_contracts() -> Result<()> {
        let image = DecodedImage::new(2, 1, Arc::from([2, 4, 6, 4, 8, 12]))?;
        let tensor = ort::value::Tensor::from_array(([1usize, 3, 1, 2], vec![0.0f32; 6]))?;
        let (shape, values) =
            processor().prepare(&image, tensor.dtype(), MediaLimits::default())?;
        assert_eq!(shape, [1, 3, 1, 2]);
        assert_eq!(values, [0.0, 1.0, 0.0, 1.0, 0.0, 0.75]);
        let mut dynamic = tensor.dtype().clone();
        if let ValueType::Tensor {
            shape,
            dimension_symbols,
            ..
        } = &mut dynamic
        {
            *shape = ort::value::Shape::new([-1, -1, -1, -1]);
            *dimension_symbols = ort::value::SymbolicDimensions::new([
                "batch".into(),
                "num_channels".into(),
                "height".into(),
                "width".into(),
            ]);
        }
        assert_eq!(
            processor()
                .prepare(&image, &dynamic, MediaLimits::default())?
                .1,
            values
        );
        let tensor = ort::value::Tensor::from_array(([1usize, 1, 2, 3], vec![0.0f32; 6]))?;
        assert_eq!(
            processor()
                .prepare(&image, tensor.dtype(), MediaLimits::default())?
                .1,
            [0.0, 0.0, 0.0, 1.0, 1.0, 0.75]
        );
        let mut invalid = processor();
        invalid.config["image_std"] = serde_json::json!([1, 0, 1]);
        assert!(invalid
            .prepare(&image, tensor.dtype(), MediaLimits::default())
            .is_err());
        invalid
            .config
            .as_object_mut()
            .context("Config object")?
            .remove("do_rescale");
        assert!(invalid
            .prepare(&image, tensor.dtype(), MediaLimits::default())
            .is_err());
        Ok(())
    }

    #[test]
    fn test_shortest_resize_center_crop_preserves_pixels_and_checks_allocation() -> Result<()> {
        let pixels: Vec<_> = (0u8..8).flat_map(|value| [value, 0, 0]).collect();
        let image = DecodedImage::new(2, 4, pixels.into())?;
        let tensor = ort::value::Tensor::from_array(([1usize, 3, 2, 2], vec![0.0f32; 12]))?;
        let mut processor = VisionProcessor {
            config: serde_json::json!({
            "do_resize":true,"size":{"shortest_edge":2},"resample":0,
            "do_center_crop":true,"crop_size":{"width":2,"height":2},
            "do_rescale":false,"do_normalize":false}),
        };
        let (_, values) = processor.prepare(&image, tensor.dtype(), MediaLimits::default())?;
        assert_eq!(&values[..4], &[2.0, 3.0, 4.0, 5.0]);
        processor.config["size"] = serde_json::json!(128);
        assert!(processor
            .prepare(&image, tensor.dtype(), MediaLimits::new(64, 64)?)
            .is_err());
        Ok(())
    }
}
