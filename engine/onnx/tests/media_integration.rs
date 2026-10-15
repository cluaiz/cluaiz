//! Actual selected image graph and the legacy driver share one typed computation.
use anyhow::{Context, Result};
use cluaiz_core::{EmbeddingDriver, Modality};
use onnx_engine::OnnxEngine;
#[test]
#[ignore = "requires a real external image package with processor/tokenizer assets"]
fn real_image_graph_signature_and_embedding_driver() -> Result<()> {
    let primary = std::path::PathBuf::from(std::env::var("CLUAIZ_ONNX_IMAGE_MODEL")?);
    let mut settings = cluaiz_core::OnnxSettings::default();
    settings.n_gpu_layers = 0;
    settings.intra_op_num_threads = 2;
    let mut engine = OnnxEngine::with_settings(settings)?;
    engine.load_embedding_package(primary.parent().context("Package parent")?, &primary)?;
    let session = engine
        .acquire_session()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    {
        let session = session
            .lock()
            .map_err(|_| anyhow::anyhow!("Test session mutex poisoned"))?;
        for input in session.inputs() {
            println!("REAL IMAGE GRAPH {}: {:?}", input.name(), input.dtype());
        }
        for output in session.outputs() {
            println!("REAL IMAGE OUTPUT {}: {:?}", output.name(), output.dtype());
        }
    }
    let image = image::RgbImage::from_pixel(32, 16, image::Rgb([127u8, 127, 127]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut bytes, image::ImageOutputFormat::Png)?;
    let embedding = engine.gen_multimodal_embedding(bytes.get_ref(), Modality::Image, None)?;
    assert!(!embedding.is_empty() && embedding.iter().all(|v| v.is_finite()));
    println!("REAL IMAGE DRIVER: {} dimensions", embedding.len());
    Ok(())
}
