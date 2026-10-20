//! Opt-in acceptance of real generated PCM, tempo and frame-boundary cancellation.
use anyhow::Result;
use cluaiz_core::media_limits::MediaLimits;
use llama_engine::native::speech::{SpeechModel, SpeechOptions};
use std::{cell::Cell, path::PathBuf, time::Instant};

#[test]
#[ignore = "requires CLUAIZ_TTS_BACKBONE, CLUAIZ_TTS_PROJECTOR, CLUAIZ_TTS_TEXT_FILE, CLUAIZ_TTS_OUT_DIR"]
fn test_native_speech_progressive_speed_cancel_recovery() -> Result<()> {
    let backbone = PathBuf::from(std::env::var("CLUAIZ_TTS_BACKBONE")?);
    let projector = PathBuf::from(std::env::var("CLUAIZ_TTS_PROJECTOR")?);
    let text = std::fs::read_to_string(std::env::var("CLUAIZ_TTS_TEXT_FILE")?)?;
    let output = PathBuf::from(std::env::var("CLUAIZ_TTS_OUT_DIR")?);
    std::fs::create_dir_all(&output)?;
    let threads = std::env::var("CLUAIZ_TTS_TEST_THREADS").ok()
        .map(|value| value.parse::<i32>()).transpose()?.unwrap_or(2);
    let gpu = std::env::var("CLUAIZ_TTS_TEST_GPU").ok()
        .map(|value| value.parse::<bool>()).transpose()?.unwrap_or(false);
    let load_start = Instant::now();
    let mut model = SpeechModel::load(&backbone, &projector, gpu, threads)?;
    println!("Native speech profile: gpu {gpu}, threads {threads}, load {:.3}s", load_start.elapsed().as_secs_f64());
    let options = SpeechOptions {
        language: Some("en".into()),
        seed: Some(42),
        ..Default::default()
    };
    assert!(model.speak("", &options).is_err());
    assert!(model
        .speak(
            &text,
            &SpeechOptions {
                speed: Some(f32::NAN),
                ..options.clone()
            }
        )
        .is_err());
    let mut normal = Vec::new();
    let rate = model.sample_rate();
    let mut count = 0;
    let start = Instant::now();
    model.speak_pcm(
        &text,
        &options,
        |pcm, actual_rate| {
            assert_eq!(actual_rate, rate);
            assert!(!pcm.is_empty() && pcm.iter().all(|v| v.is_finite()));
            println!(
                "Native speech chunk {count}: {} samples at {:.3}s",
                pcm.len(),
                start.elapsed().as_secs_f64()
            );
            normal.extend_from_slice(pcm);
            count += 1;
            Ok(())
        },
        || false,
    )?;
    assert!(count > 1, "fixture must produce PCM before the final flush");
    assert!(normal.iter().any(|v| v.abs() > 0.001));
    println!("Native speech normal complete: {} samples, rate {rate}, synthesis {:.3}s", normal.len(), start.elapsed().as_secs_f64());
    println!("Native speech stages: {}", serde_json::to_string(&model.timings())?);
    std::fs::write(output.join("native-stages.json"), serde_json::to_vec_pretty(&model.timings())?)?;
    std::fs::write(
        output.join("gguf-stream.wav"),
        cluaiz_media::wav::encode_pcm16(&normal, rate, options.limits)?,
    )?;
    // Upstream clip.cpp reseeds Fast AR only when the seed changes, and its RNG survives an
    // utterance reset. Compare pace on fresh owners so both runs generate the same source codes.
    drop(model);
    let mut model = SpeechModel::load(&backbone, &projector, gpu, threads)?;
    let mut fast = Vec::new();
    model.speak_pcm(
        &text,
        &SpeechOptions {
            speed: Some(1.5),
            ..options.clone()
        },
        |pcm, _| {
            fast.extend_from_slice(pcm);
            Ok(())
        },
        || false,
    )?;
    let ratio = fast.len() as f64 / normal.len() as f64;
    assert!(
        (ratio - 1.0 / 1.5).abs() < 0.04,
        "tempo duration ratio {ratio}"
    );
    std::fs::write(
        output.join("gguf-speed.wav"),
        cluaiz_media::wav::encode_pcm16(&fast, rate, options.limits)?,
    )?;
    let delivered = Cell::new(false);
    let error = model
        .speak_pcm(
            &text,
            &options,
            |_, _| {
                delivered.set(true);
                Ok(())
            },
            || delivered.get(),
        )
        .unwrap_err();
    assert!(delivered.get() && error.to_string().contains("cancelled"));
    assert!(model
        .speak(
            &text,
            &SpeechOptions {
                max_frames: Some(1),
                ..options.clone()
            }
        )
        .is_err());
    let recovered = model.speak(&text, &options)?;
    assert!(recovered.len() > 44);
    std::fs::write(output.join("gguf-recovery.wav"), recovered)?;
    println!("Native speech passed: {count} chunks, {} normal samples, {} fast samples, rate {rate}, ratio {ratio:.4}; cancellation, frame limit and recovery passed", normal.len(), fast.len());
    // A small decoded budget fails rather than accumulating an unbounded PCM output.
    let limits = MediaLimits::new(1024, 1024)?;
    let error = model
        .speak(&text, &SpeechOptions { limits, ..options })
        .unwrap_err();
    assert!(
        error
            .chain()
            .any(|e| e.to_string().contains("configured limit")),
        "{error:#}"
    );
    Ok(())
}
