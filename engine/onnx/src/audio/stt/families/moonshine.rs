//! Moonshine ONNX family adapter (`input_values` raw-waveform encoder + RoPE autoregressive decoder).
//!
//! Moonshine (`UsefulSensors/moonshine`, `onnx-community/moonshine-tiny-ONNX`,
//! `onnx-community/moonshine-base-ONNX`) feeds raw 16 kHz mono PCM waveforms directly into the
//! convolutional encoder stem (`input_values: float32[1, num_samples]`) without padding to a fixed
//! 30-second Mel-spectrogram window, scaling encoder compute proportionally to the audio duration.

use super::{SttEncoderKind, SttFamilyProfile};
use crate::engine::OnnxEngine;
use anyhow::{anyhow, bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use ort::session::Session;
use ort::value::Value;

/// Default maximum audio duration (64 seconds) when `preprocessor_config.json` does not specify an
/// explicit `n_samples` / `max_length` limit.
const DEFAULT_MOONSHINE_MAX_SECONDS: usize = 64;

/// Resolves and validates the Moonshine processor and special-token contract from
/// `preprocessor_config.json`, `config.json`, `generation_config.json`, and `tokenizer.json`.
pub fn resolve_moonshine_profile(engine: &OnnxEngine) -> Result<SttFamilyProfile> {
    let dir = engine
        .model_dir
        .as_ref()
        .context("STT model directory missing")?;
    let processor: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("preprocessor_config.json"))?)?;

    let sample_rate = processor
        .get("sampling_rate")
        .or_else(|| processor.get("sample_rate"))
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .context("Moonshine STT processor requires positive sampling_rate")?;

    let do_normalize = processor
        .get("do_normalize")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let max_samples = processor
        .get("n_samples")
        .or_else(|| processor.get("max_length"))
        .and_then(|v| v.as_u64())
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
        .unwrap_or_else(|| (sample_rate as usize).saturating_mul(DEFAULT_MOONSHINE_MAX_SECONDS));

    let mut decoder_start_token: Option<i64> = None;
    let mut eos_token: Option<i64> = None;
    let mut max_target_positions: Option<usize> = None;

    for filename in ["config.json", "generation_config.json", "tokenizer_config.json"] {
        let path = dir.join(filename);
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if decoder_start_token.is_none() {
                    decoder_start_token = json
                        .get("decoder_start_token_id")
                        .or_else(|| json.get("bos_token_id"))
                        .and_then(|v| v.as_i64());
                }
                if eos_token.is_none() {
                    eos_token = json
                        .get("eos_token_id")
                        .and_then(|v| v.as_i64().or_else(|| v.as_array()?.first()?.as_i64()));
                }
                if max_target_positions.is_none() {
                    max_target_positions = json
                        .get("max_position_embeddings")
                        .or_else(|| json.get("max_target_positions"))
                        .or_else(|| json.get("max_length"))
                        .and_then(|v| v.as_u64())
                        .and_then(|v| usize::try_from(v).ok())
                        .filter(|v| *v > 4);
                }
            }
        }
    }

    if let Some(tokenizer) = &engine.tokenizer {
        if decoder_start_token.is_none() {
            for candidate in ["<s>", "<|startoftranscript|>", "<bos>"] {
                if let Some(id) = tokenizer.token_to_id(candidate) {
                    decoder_start_token = Some(i64::from(id));
                    break;
                }
            }
        }
        if eos_token.is_none() {
            for candidate in ["</s>", "<|endoftext|>", "<eos>"] {
                if let Some(id) = tokenizer.token_to_id(candidate) {
                    eos_token = Some(i64::from(id));
                    break;
                }
            }
        }
    }

    let decoder_start_token =
        decoder_start_token.context("Moonshine STT requires decoder_start_token_id or bos_token_id")?;
    let eos_token = eos_token.context("Moonshine STT requires eos_token_id")?;
    let max_target_positions = max_target_positions.unwrap_or(512);
    if max_target_positions <= 4 {
        bail!("Moonshine STT max_position_embeddings must exceed 4");
    }

    Ok(SttFamilyProfile {
        sample_rate,
        max_samples,
        encoder_kind: SttEncoderKind::RawWaveform { do_normalize },
        max_target_positions,
        decoder_start_token,
        eos_token,
        transcribe_token: None,
        translate_token: None,
        no_timestamps_token: None,
        // Moonshine paper / reference implementation bounds generation at ~6.5 tokens per second of audio.
        tokens_per_second_budget: 6.5,
        min_token_budget: 24,
    })
}

/// Normalizes (if configured in `preprocessor_config.json`) and feeds the 16 kHz mono PCM waveform
/// `[1, num_samples]` directly into the Moonshine ONNX encoder (`input_values` -> `last_hidden_state`).
pub fn encode_moonshine_waveform(
    encoder: &mut Session,
    pcm_samples: &[f32],
    profile: &SttFamilyProfile,
    limits: MediaLimits,
) -> Result<(Vec<usize>, Vec<f32>)> {
    let SttEncoderKind::RawWaveform { do_normalize } = profile.encoder_kind else {
        bail!("Moonshine encoder requires RawWaveform profile");
    };
    if pcm_samples.is_empty() {
        bail!("Moonshine STT requires non-empty audio samples");
    }

    let wav_bytes = pcm_samples
        .len()
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| anyhow!("Moonshine waveform byte size overflow"))?;
    limits.check_decoded_bytes(wav_bytes as u64)?;

    let samples: Vec<f32> = if do_normalize {
        let n = pcm_samples.len() as f32;
        let mean = pcm_samples.iter().copied().sum::<f32>() / n;
        let var = pcm_samples
            .iter()
            .map(|&x| {
                let d = x - mean;
                d * d
            })
            .sum::<f32>()
            / n;
        let std_dev = (var + 1e-7).sqrt();
        pcm_samples.iter().map(|&x| (x - mean) / std_dev).collect()
    } else {
        pcm_samples.to_vec()
    };

    let num_samples = samples.len();
    let input_values = Value::from_array(([1usize, num_samples], samples))?;
    let outputs = encoder.run(ort::inputs!["input_values" => input_values])?;
    let output = outputs
        .values()
        .next()
        .ok_or_else(|| anyhow!("Moonshine STT encoder output missing"))?;
    let (shape, data) = output.try_extract_tensor::<f32>()?;
    Ok((
        shape.iter().map(|&n| n as usize).collect::<Vec<_>>(),
        data.to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moonshine_profile_parses_config_and_scales_token_budget_with_audio_duration() {
        let dir = std::env::temp_dir().join(format!("cluaiz-moonshine-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("preprocessor_config.json"),
            r#"{"feature_extractor_type":"MoonshineFeatureExtractor","sampling_rate":16000,"do_normalize":false}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("config.json"),
            r#"{"model_type":"moonshine","bos_token_id":1,"decoder_start_token_id":1,"eos_token_id":2,"max_position_embeddings":512}"#,
        )
        .unwrap();

        let mut engine = OnnxEngine::with_settings(cluaiz_core::OnnxSettings {
            n_gpu_layers: 0,
            ..Default::default()
        })
        .unwrap();
        engine.model_dir = Some(dir.clone());

        let profile = resolve_moonshine_profile(&engine).unwrap();
        assert_eq!(profile.sample_rate, 16000);
        assert_eq!(profile.decoder_start_token, 1);
        assert_eq!(profile.eos_token, 2);
        assert_eq!(profile.max_target_positions, 512);
        assert_eq!(
            profile.encoder_kind,
            SttEncoderKind::RawWaveform {
                do_normalize: false
            }
        );
        assert!(profile.transcribe_token.is_none());

        // 2s audio -> ceil(2 * 6.5 + 24) = 37 steps; 10s audio -> ceil(10 * 6.5 + 24) = 89 steps
        let steps_2s = profile.max_decode_steps(2.0);
        let steps_10s = profile.max_decode_steps(10.0);
        assert_eq!(steps_2s, 37);
        assert_eq!(steps_10s, 89);
        assert!(steps_2s < steps_10s);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn build_raw_waveform_identity_onnx() -> Vec<u8> {
        fn varint(mut v: usize) -> Vec<u8> {
            let mut out = Vec::new();
            while v >= 128 {
                out.push(((v & 0x7f) as u8) | 0x80);
                v >>= 7;
            }
            out.push(v as u8);
            out
        }
        fn msg(field_num: u8, payload: &[u8]) -> Vec<u8> {
            let mut out = vec![(field_num << 3) | 2];
            out.extend_from_slice(&varint(payload.len()));
            out.extend_from_slice(payload);
            out
        }
        fn value_info_1x4(name: &str) -> Vec<u8> {
            let dim1 = msg(1, &[8, 1]);
            let dim4 = msg(1, &[8, 4]);
            let mut shape = Vec::new();
            shape.extend_from_slice(&dim1);
            shape.extend_from_slice(&dim4);
            let mut tensor_type = vec![8, 1]; // elem_type = FLOAT (1)
            tensor_type.extend_from_slice(&msg(2, &shape));
            let type_proto = msg(1, &tensor_type);
            let mut vi = msg(1, name.as_bytes());
            vi.extend_from_slice(&msg(2, &type_proto));
            vi
        }
        let mut node = msg(1, b"input_values");
        node.extend_from_slice(&msg(2, b"last_hidden_state"));
        node.extend_from_slice(&msg(4, b"Identity"));

        let mut graph = msg(1, &node);
        graph.extend_from_slice(&msg(2, b"moonshine_test_graph"));
        graph.extend_from_slice(&msg(11, &value_info_1x4("input_values")));
        graph.extend_from_slice(&msg(12, &value_info_1x4("last_hidden_state")));

        let mut model = vec![8, 8]; // ir_version = 8
        model.extend_from_slice(&msg(7, &graph));
        model.extend_from_slice(&msg(8, &[16, 13])); // opset_import version = 13
        model
    }

    #[test]
    fn encode_moonshine_waveform_executes_real_onnx_graph_and_normalizes() {
        let onnx_bytes = build_raw_waveform_identity_onnx();
        let mut session = Session::builder()
            .unwrap()
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])
            .unwrap()
            .commit_from_memory(&onnx_bytes)
            .unwrap();

        let mut profile = SttFamilyProfile {
            sample_rate: 16000,
            max_samples: 16000 * 64,
            encoder_kind: SttEncoderKind::RawWaveform {
                do_normalize: false,
            },
            max_target_positions: 512,
            decoder_start_token: 1,
            eos_token: 2,
            transcribe_token: None,
            translate_token: None,
            no_timestamps_token: None,
            tokens_per_second_budget: 6.5,
            min_token_budget: 24,
        };

        let pcm = [0.25f32, -0.25, 0.5, -0.5];
        let (shape, out) =
            encode_moonshine_waveform(&mut session, &pcm, &profile, MediaLimits::default())
                .unwrap();
        assert_eq!(shape, vec![1, 4]);
        assert_eq!(out, pcm);

        profile.encoder_kind = SttEncoderKind::RawWaveform { do_normalize: true };
        let (norm_shape, norm_out) =
            encode_moonshine_waveform(&mut session, &pcm, &profile, MediaLimits::default())
                .unwrap();
        assert_eq!(norm_shape, vec![1, 4]);
        let mean: f32 = norm_out.iter().sum::<f32>() / (norm_out.len() as f32);
        assert!(mean.abs() < 1e-5, "normalized mean={mean}");
    }
}
