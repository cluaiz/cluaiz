use super::*;

fn tone() -> Vec<f32> {
    (0..32_000).map(|n| (n as f32 * std::f32::consts::TAU * 200.0 / 16_000.0).sin() * 0.5).collect()
}

fn transform(input: &[f32], speed: f32, chunk: usize) -> Result<Vec<f32>> {
    let mut tempo = Tempo::new(16_000, speed, MediaLimits::default())?;
    let mut out = Vec::new();
    for part in input.chunks(chunk) { out.extend(tempo.write(part)?); }
    out.extend(tempo.finish()?);
    assert!(tempo.finish()?.is_empty());
    assert!(tempo.write(&[0.0]).is_err());
    Ok(out)
}

#[test]
fn test_tempo_speed_identity_duration_pitch_and_streaming() -> Result<()> {
    let input = tone();
    assert_eq!(transform(&input, 1.0, 317)?, input);
    for speed in [0.5, 0.75, 1.5, 2.0, 4.0] {
        let out = transform(&input, speed, 317)?;
        let whole = transform(&input, speed, input.len())?;
        assert!(out == whole, "stream partition at {speed}: chunked {} samples, whole {} samples, first difference {:?}",
            out.len(), whole.len(), out.iter().zip(&whole).position(|(a, b)| a != b));
        let expected = input.len() as f32 / speed;
        assert!((out.len() as f32 - expected).abs() < 0.02 * expected + 1.0, "duration at {speed}: {}", out.len());
        // Zero-crossing diagnostic on the center: tempo must not act like rate/pitch resampling.
        let middle = &out[out.len() / 4..out.len() * 3 / 4];
        let crossings = middle.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let hz = crossings as f32 * 16_000.0 / middle.len() as f32;
        assert!((hz - 200.0).abs() < 12.0, "pitch at speed {speed}: {hz}");
    }
    Ok(())
}

#[test]
fn test_tempo_rejects_invalid_input_and_budget() -> Result<()> {
    for speed in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(Tempo::new(16_000, speed, MediaLimits::default()).is_err());
    }
    assert!(Tempo::new(0, 1.0, MediaLimits::default()).is_err());
    let mut tempo = Tempo::new(16_000, 1.0, MediaLimits::new(16, 16)?)?;
    assert!(tempo.write(&[f32::NAN]).is_err());
    assert!(tempo.write(&[0.0; 2]).is_err());
    Ok(())
}
