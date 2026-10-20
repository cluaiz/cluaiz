use super::*;
use std::{io::Cursor, path::PathBuf};

#[test]
fn test_audio8_cache_budget_overflow_and_manifest_contract() -> Result<()> {
    assert!(Caches::bytes(usize::MAX, 2, 2048, 64).is_err());
    assert!(Caches::new(24, 2, 2048, 64, MediaLimits::new(1024, 1024)?).is_err());
    let protocol: Protocol = serde_json::from_str(include_str!("protocol.json"))?;
    assert_eq!(
        protocol
            .prompt
            .iter()
            .filter(|s| s.as_str() == "{text}")
            .count(),
        1
    );
    assert!(protocol.chunk_frames > 0 && protocol.repeat_window > 0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(package::asset(root, "../core/Cargo.toml").is_err());
    assert!(package::asset(
        root,
        root.parent()
            .context("crate parent")?
            .join("core/Cargo.toml")
            .to_str()
            .context("UTF8 fixture path")?
    )
    .is_err());
    assert!(package::asset(root, "not-an-installed-audio8-asset").is_err());
    Ok(())
}

/// Runs only against an explicitly supplied real package/text; never part of the production CLI.
#[test]
#[ignore = "requires CLUAIZ_AUDIO8_PACKAGE, CLUAIZ_TTS_TEXT_FILE and CLUAIZ_TTS_OUT_DIR"]
fn test_native_audio8_pcm_chunks_recovery_and_cancellation() -> Result<()> {
    let dir = PathBuf::from(std::env::var("CLUAIZ_AUDIO8_PACKAGE")?);
    let text = std::fs::read_to_string(std::env::var("CLUAIZ_TTS_TEXT_FILE")?)?;
    let out = PathBuf::from(std::env::var("CLUAIZ_TTS_OUT_DIR")?);
    std::fs::create_dir_all(&out)?;
    let mut settings = cluaiz_core::Settings::current().onnx;
    settings.n_gpu_layers = 0;
    settings.intra_op_num_threads = 2;
    let engine = crate::OnnxEngine::with_settings(settings)?;
    let mut runtime = Runtime::load(&engine, &dir)?;
    let limits = MediaLimits::default();
    let voice = std::env::var("CLUAIZ_AUDIO8_VOICE").ok();
    assert!(runtime.synthesize("", None, 1.0, limits, None).is_err());
    assert!(runtime
        .synthesize(&text, None, 1.0, MediaLimits::new(1024, 1024)?, None)
        .is_err());
    let mut parts = Vec::new();
    let max_parts = std::env::var("CLUAIZ_TTS_MAX_PARTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    let started = std::time::Instant::now();
    let uri = runtime.synthesize(
        &text,
        voice.as_deref(),
        1.0,
        limits,
        Some(&mut |uri| {
            println!(
                "Audio8 chunk {} at {:.3}s",
                parts.len(),
                started.elapsed().as_secs_f64()
            );
            std::fs::write(
                out.join(format!("audio8-part-{}.wav", parts.len())),
                bytes(&uri).expect("native WAV chunk"),
            )
            .expect("save native diagnostic chunk");
            parts.push(uri);
            !max_parts.is_some_and(|limit| parts.len() >= limit)
        }),
    )?;
    let wav = bytes(&uri)?;
    let reader = hound::WavReader::new(Cursor::new(&wav))?;
    assert_eq!(
        reader.spec().sample_rate,
        runtime.manifest.codec_sample_rate
    );
    let samples = reader
        .into_samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let joined = parts
        .iter()
        .map(|s| -> Result<Vec<i16>> {
            Ok(hound::WavReader::new(Cursor::new(bytes(s)?))?
                .into_samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .collect::<Result<Vec<_>>>()?
        .concat();
    assert_eq!(joined, samples);
    assert!(!samples.is_empty() && samples.iter().any(|v| v.abs() > 32));
    println!(
        "Audio8 complete: {} chunks, {} samples, {} Hz, {:.3}s elapsed",
        parts.len(),
        samples.len(),
        runtime.manifest.codec_sample_rate,
        started.elapsed().as_secs_f64()
    );
    std::fs::write(out.join("audio8-native.wav"), wav)?;
    let cancelled = runtime
        .synthesize(&text, voice.as_deref(), 1.0, limits, Some(&mut |_| false))
        .unwrap_err();
    assert!(cancelled.to_string().contains("cancelled"));
    let recovered = runtime.synthesize(&text, voice.as_deref(), 1.5, limits, None)?;
    let recovered_wav = bytes(&recovered)?;
    assert!(recovered_wav.len() > 44);
    std::fs::write(out.join("audio8-speed-recovery.wav"), recovered_wav)?;
    println!("Audio8 cancellation then speed1.5 recovery passed");
    Ok(())
}

fn bytes(uri: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.decode(
        uri.strip_prefix("data:audio/wav;base64,")
            .context("WAV URI missing")?,
    )?)
}

/// Prepare a genuine fixture with the package's codec encoder, never invented codebook tokens.
#[test]
#[ignore = "requires CLUAIZ_AUDIO8_PACKAGE, CLUAIZ_AUDIO8_ENCODER, CLUAIZ_TTS_REFERENCE_WAV, CLUAIZ_TTS_TEXT_FILE"]
fn test_register_audio8_reference_fixture() -> Result<()> {
    use cluaiz_media::resample::{prepare_mono, resample_linear, PeakNormalization};
    use ort::value::{TensorElementType, TensorRef, ValueType};
    let dir = PathBuf::from(std::env::var("CLUAIZ_AUDIO8_PACKAGE")?);
    let package = Package::read(&dir)?;
    let limits = MediaLimits::default();
    let wav = std::fs::read(std::env::var("CLUAIZ_TTS_REFERENCE_WAV")?)?;
    let audio = cluaiz_media::audio::decode_audio(wav, limits)?;
    let mono = prepare_mono(&audio, PeakNormalization::Preserve, limits)?;
    let audio = resample_linear(&mono, package.manifest.codec_sample_rate, limits)?;
    let hop = package.manifest.codec_hop_length;
    let padded = audio.samples().len().div_ceil(hop) * hop;
    let mut pcm = audio.samples().to_vec();
    pcm.resize(padded, 0.0);
    let mut settings = cluaiz_core::Settings::current().onnx;
    settings.n_gpu_layers = 0;
    settings.intra_op_num_threads = 2;
    let engine = crate::OnnxEngine::with_settings(settings)?;
    let mut encoder = engine.build_session(Path::new(&std::env::var("CLUAIZ_AUDIO8_ENCODER")?))?;
    let dtype = match encoder.inputs()[0].dtype() {
        ValueType::Tensor { ty, .. } => *ty,
        _ => bail!("Invalid encoder input"),
    };
    let half: Vec<_> = pcm.iter().map(|&v| half::f16::from_f32(v)).collect();
    let result = match dtype {
        TensorElementType::Float16 => encoder.run(
            ort::inputs!["audio" => TensorRef::from_array_view(([1,1,padded], half.as_slice()))?],
        )?,
        TensorElementType::Float32 => encoder.run(
            ort::inputs!["audio" => TensorRef::from_array_view(([1,1,padded], pcm.as_slice()))?],
        )?,
        _ => bail!("Unsupported encoder input dtype"),
    };
    let (shape, codes) = result[0].try_extract_tensor::<i64>()?;
    let books = package.manifest.num_codebooks;
    assert!(codes.len() > 0 && codes.len() % books == 0, "{shape:?}");
    let frames = codes.len() / books;
    assert!(
        shape.as_ref() == [1, books as i64, frames as i64]
            || shape.as_ref() == [books as i64, frames as i64]
    );
    let name = std::env::var("CLUAIZ_AUDIO8_VOICE")?;
    assert!(!name.contains(['/', '\\', ':']) && name != "." && name != "..");
    let destination = dir.join("voices").join(&name);
    assert!(
        !destination.exists(),
        "reference fixture must not overwrite a profile"
    );
    std::fs::create_dir_all(&destination)?;
    let mut header =
        format!("{{'descr': '<u2', 'fortran_order': False, 'shape': ({books}, {frames}), }}");
    let header_len = (10 + header.len() + 1).div_ceil(64) * 64 - 10;
    header.extend(std::iter::repeat_n(' ', header_len - header.len() - 1));
    header.push('\n');
    let mut npy = b"\x93NUMPY\x01\x00".to_vec();
    npy.extend(u16::try_from(header.len())?.to_le_bytes());
    npy.extend(header.as_bytes());
    for &code in codes {
        assert!((0..package.manifest.codebook_size as i64).contains(&code));
        npy.extend(u16::try_from(code)?.to_le_bytes());
    }
    std::fs::write(destination.join("codes.npy"), npy)?;
    let text = std::fs::read_to_string(std::env::var("CLUAIZ_TTS_TEXT_FILE")?)?;
    std::fs::write(
        destination.join("meta.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "name": name, "reference_text": text, "language": "en", "model_fingerprint": package.manifest.model_fingerprint,
            "source_kind": "native_test_codec_registration"
        }))?,
    )?;
    println!(
        "Genuine codec reference saved: {books} books, {frames} frames, {}",
        destination.display()
    );
    Ok(())
}

#[test]
#[ignore = "requires CLUAIZ_AUDIO8_PACKAGE, CLUAIZ_AUDIO8_VOICE and CLUAIZ_TTS_OUT_DIR"]
fn test_audio8_reference_codec_roundtrip() -> Result<()> {
    use ort::value::TensorRef;
    let dir = PathBuf::from(std::env::var("CLUAIZ_AUDIO8_PACKAGE")?);
    let p = Package::read(&dir)?;
    let limits = MediaLimits::default();
    let reference = voice::load(
        &dir,
        &std::env::var("CLUAIZ_AUDIO8_VOICE")?,
        &p.manifest,
        limits,
    )?;
    let mut settings = cluaiz_core::Settings::current().onnx;
    settings.n_gpu_layers = 0;
    settings.intra_op_num_threads = 2;
    let engine = crate::OnnxEngine::with_settings(settings)?;
    let mut decoder = engine.build_session(&p.decoder)?;
    let result = decoder.run(ort::inputs!["codes" => TensorRef::from_array_view(([1,p.manifest.num_codebooks,reference.frames],reference.codes.as_slice()))?])?;
    let (_, pcm) = result[0].try_extract_tensor::<f32>()?;
    assert!(pcm.iter().all(|v| v.is_finite()) && !pcm.is_empty());
    let out = PathBuf::from(std::env::var("CLUAIZ_TTS_OUT_DIR")?);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("audio8-codec-roundtrip.wav"),
        cluaiz_media::wav::encode_pcm16(pcm, p.manifest.codec_sample_rate, limits)?,
    )?;
    println!(
        "Audio8 genuine reference codec roundtrip: {} PCM samples at {} Hz",
        pcm.len(),
        p.manifest.codec_sample_rate
    );
    Ok(())
}

#[test]
#[ignore = "requires CLUAIZ_AUDIO8_PACKAGE, CLUAIZ_TTS_TEXT_FILE, CLUAIZ_TTS_OUT_DIR"]
fn test_audio8_prefill_differential_fixture() -> Result<()> {
    let dir = PathBuf::from(std::env::var("CLUAIZ_AUDIO8_PACKAGE")?);
    let text = std::fs::read_to_string(std::env::var("CLUAIZ_TTS_TEXT_FILE")?)?;
    let voice = std::env::var("CLUAIZ_AUDIO8_VOICE").ok();
    let out = PathBuf::from(std::env::var("CLUAIZ_TTS_OUT_DIR")?);
    std::fs::create_dir_all(&out)?;
    let mut settings = cluaiz_core::Settings::current().onnx;
    settings.n_gpu_layers = 0;
    settings.intra_op_num_threads = 2;
    let engine = crate::OnnxEngine::with_settings(settings)?;
    let mut runtime = Runtime::load(&engine, &dir)?;
    let limits = MediaLimits::default();
    let (codes, prompt_len) = runtime.prompt(&text, voice.as_deref(), limits)?;
    let m = &runtime.manifest;
    let mut slow_cache = Caches::new(
        m.num_layers,
        m.n_local_heads,
        m.max_seq_len,
        m.head_dim,
        limits,
    )?;
    let positions: Vec<_> = (0..prompt_len).map(|p| p as i64).collect();
    let (logits, hidden) = steps::slow(&mut runtime.slow, &codes, &positions, &mut slow_cache, m)?;
    let semantic = logits[..m.codebook_size]
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .context("empty logits")?
        .0;
    let mut fast_cache = Caches::new(
        m.num_fast_layers,
        m.fast_n_local_heads,
        m.num_codebooks,
        m.fast_head_dim,
        limits,
    )?;
    steps::fast(&mut runtime.fast, &hidden, 0, true, 0, &mut fast_cache, m)?;
    let fast = steps::fast(
        &mut runtime.fast,
        &hidden,
        semantic as i64,
        false,
        1,
        &mut fast_cache,
        m,
    )?;
    let hidden: Vec<_> = hidden.iter().map(|v| v.to_bits()).collect();
    std::fs::write(
        out.join("rust-prefill.json"),
        serde_json::to_vec(&serde_json::json!({
            "promptLen":prompt_len,"ids":&codes[..prompt_len],"codes":codes,"logits":logits,"hidden":hidden,
            "semantic":semantic,"fast_logits":fast
        }))?,
    )?;
    println!("{}", ort::info());
    println!(
        "Captured settings: {}",
        serde_json::to_string(&engine.settings)?
    );
    println!("Saved genuine Rust prefill/first Fast AR logits: {prompt_len} prompt positions, semantic argmax {semantic}");
    Ok(())
}
