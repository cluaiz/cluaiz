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

#[test]
#[ignore = "requires CLUAIZ_TTS_TEST_PACKAGES JSON graph paths and CLUAIZ_TTS_TEST_TEXT"]
fn real_installed_tts_packages_produce_nonempty_waveforms() -> Result<()> {
    use base64::Engine;
    use onnx_engine::media::{AudioInput, AudioOutput, AudioTask, SttOptions};
    let packages: Vec<std::path::PathBuf> =
        serde_json::from_str(&std::env::var("CLUAIZ_TTS_TEST_PACKAGES")?)?;
    anyhow::ensure!(!packages.is_empty(), "No TTS packages supplied");
    let text = std::env::var("CLUAIZ_TTS_TEST_TEXT")?;
    let output = std::path::PathBuf::from(std::env::var("CLUAIZ_TTS_TEST_OUTPUT")?);
    std::fs::create_dir_all(&output)?;
    let mut failures = Vec::new();
    for (index, primary) in packages.iter().enumerate() {
        let dir = primary.parent().context("TTS package parent")?;
        let name = dir
            .file_name()
            .context("TTS package name")?
            .to_string_lossy();
        let result = (|| -> Result<()> {
            let mut settings = cluaiz_core::OnnxSettings::default();
            settings.n_gpu_layers = 0;
            settings.intra_op_num_threads = 2;
            let mut engine = OnnxEngine::with_settings(settings)?;
            engine.load_audio_package(
                dir,
                primary,
                std::slice::from_ref(primary),
                AudioTask::TextToSpeech,
            )?;
            let empty = engine.execute_prepared_audio(
                AudioTask::TextToSpeech,
                &AudioInput::Text(String::new()),
                &SttOptions::default(),
                Default::default(),
                None,
            );
            anyhow::ensure!(empty.is_err(), "Empty TTS input was accepted");
            let audio = engine.execute_prepared_audio(
                AudioTask::TextToSpeech,
                &AudioInput::Text(text.clone()),
                &SttOptions::default(),
                Default::default(),
                None,
            )?;
            let AudioOutput::WavDataUri(uri) = audio else {
                anyhow::bail!("Expected actual audio result");
            };
            let bytes = base64::engine::general_purpose::STANDARD.decode(
                uri.strip_prefix("data:audio/wav;base64,")
                    .context("WAV URI prefix")?,
            )?;
            let mut reader = hound::WavReader::new(std::io::Cursor::new(&bytes))?;
            let spec = reader.spec();
            let samples = reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()?;
            anyhow::ensure!(
                !samples.is_empty() && samples.iter().any(|sample| *sample != 0),
                "TTS produced empty/silent waveform"
            );
            let wav = output.join(format!("tts-{index}.wav"));
            std::fs::write(&wav, &bytes)?;
            println!(
                "REAL TTS WAV: package={name}; samples={}; sample_rate={}; channels={}; output={}",
                samples.len(),
                spec.sample_rate,
                spec.channels,
                wav.display()
            );
            Ok(())
        })();
        if let Err(error) = result {
            println!("REAL TTS FAILURE: package={name}; error={error:#}");
            failures.push(name.into_owned());
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "TTS packages failing: {}",
        failures.join(", ")
    );
    Ok(())
}
