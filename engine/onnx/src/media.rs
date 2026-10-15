//! Typed ONNX audio entry points and validated session/processor contracts.
use crate::{engine::try_directml_session, OnnxEngine};
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedAudio, media_limits::MediaLimits};
use ort::{
    session::Session,
    value::{TensorElementType, ValueType},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTask {
    SpeechToText,
    SpeechTranslation,
    TextToSpeech,
}

impl AudioTask {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "speech_to_text" | "automatic_speech_recognition" => Ok(Self::SpeechToText),
            "speech_translation" => Ok(Self::SpeechTranslation),
            "text_to_speech" | "voice_synthesis" => Ok(Self::TextToSpeech),
            _ => bail!("Unsupported ONNX audio task: {value}"),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpeechToText => "speech_to_text",
            Self::SpeechTranslation => "speech_translation",
            Self::TextToSpeech => "text_to_speech",
        }
    }
    pub fn slot(self) -> &'static str {
        if self == Self::TextToSpeech {
            "tts_slot"
        } else {
            "stt_slot"
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SttOptions {
    pub language: Option<String>,
    pub translate: bool,
}

#[derive(Debug, Clone)]
pub enum AudioInput {
    Text(String),
    Pcm(DecodedAudio),
}

#[derive(Debug, Clone)]
pub enum AudioOutput {
    Transcript(String),
    WavDataUri(String),
}

impl OnnxEngine {
    /// One auxiliary/session probing policy, using this owner's captured settings.
    pub fn build_session(&self, path: &Path) -> Result<Session> {
        let settings = &self.settings;
        let builder = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_intra_threads(settings.intra_op_num_threads)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_deterministic_compute(settings.use_deterministic_compute)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let cpu = ort::ep::CPU::default()
            .with_arena_allocator(settings.enable_cpu_mem_arena)
            .build();
        if settings.n_gpu_layers != 0 {
            let strategy = match settings.arena_extend_strategy.as_str() {
                "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
            };
            let mut cuda = ort::ep::CUDA::default().with_arena_extend_strategy(strategy);
            if settings.gpu_mem_limit_bytes > 0 {
                cuda = cuda.with_memory_limit(settings.gpu_mem_limit_bytes);
            }
            if let Ok(mut attempt) = builder
                .clone()
                .with_execution_providers([cuda.build().error_on_failure(), cpu.clone()])
            {
                match attempt.commit_from_file(path) {
                    Ok(session) => return Ok(session),
                    Err(error) => {
                        tracing::warn!(%error, "CUDA session creation failed; trying fallback")
                    }
                }
            }
            if let Some(session) = try_directml_session(&builder, path, &cpu) {
                return Ok(session);
            }
        }
        builder
            .with_execution_providers([cpu.error_on_failure()])
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(path)
            .with_context(|| format!("ONNX session: {}", path.display()))
    }

    /// Load only graphs from the selected package. STT accepts an uncached
    /// encoder/decoder pair, not guessed KV tensors for an arbitrary topology.
    pub fn load_audio_package(
        &mut self,
        dir: &Path,
        primary: &Path,
        graphs: &[PathBuf],
        task: AudioTask,
    ) -> Result<()> {
        self.model_dir = Some(dir.to_path_buf());
        if task == AudioTask::TextToSpeech {
            return self.load_model(primary.to_str().context("Model path is not UTF-8")?, 1);
        }
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!("STT tokenizer: {e}"))?;
        self.tokenizer = Some(Arc::new(tokenizer));
        let config = stt_config(self)?;
        let mut decoder = None;
        let mut encoder = None;
        for path in graphs {
            let session = self.build_session(path)?;
            if session.inputs().len() == 1 && session.inputs()[0].name() == "input_features" {
                validate_tensor(
                    session.inputs()[0].dtype(),
                    TensorElementType::Float32,
                    &[1, config.n_mels as i64, config.max_frames as i64],
                )?;
                if session.outputs().len() != 1 {
                    bail!("STT encoder requires one hidden-state output");
                }
                validate_tensor(
                    session.outputs()[0].dtype(),
                    TensorElementType::Float32,
                    &[1, -1, -1],
                )?;
                if encoder.replace(session).is_some() {
                    bail!("Ambiguous STT encoder graphs");
                }
            } else if is_uncached_decoder(&session) {
                for input in session.inputs() {
                    if input.name() == "input_ids" {
                        validate_tensor(input.dtype(), TensorElementType::Int64, &[1, -1])?;
                    } else {
                        validate_tensor(input.dtype(), TensorElementType::Float32, &[1, -1, -1])?;
                    }
                }
                let logits = session
                    .outputs()
                    .iter()
                    .find(|o| o.name() == "logits")
                    .context("STT logits output missing")?;
                validate_tensor(logits.dtype(), TensorElementType::Float32, &[1, -1, -1])?;
                if decoder.replace(session).is_some() {
                    bail!("Ambiguous uncached STT decoder graphs");
                }
            }
        }
        let encoder = encoder.context("No supported STT encoder graph in selected package")?;
        let decoder = decoder.context("No supported uncached STT decoder graph; cached topologies require a separate contract")?;
        let hidden = decoder
            .inputs()
            .iter()
            .find(|i| i.name() == "encoder_hidden_states")
            .context("STT hidden-state input missing")?;
        let ValueType::Tensor { shape, .. } = encoder.outputs()[0].dtype() else {
            bail!("Invalid encoder output");
        };
        validate_tensor(hidden.dtype(), TensorElementType::Float32, shape)?;
        self.encoder_session = Some(Arc::new(Mutex::new(encoder)));
        self.session_pool = vec![Arc::new(Mutex::new(decoder))];
        Ok(())
    }

    pub fn execute_prepared_audio(
        &self,
        task: AudioTask,
        input: &AudioInput,
        options: &SttOptions,
        limits: MediaLimits,
        callback: Option<&mut (dyn FnMut(String) -> bool + Send)>,
    ) -> Result<AudioOutput> {
        match (task, input) {
            (AudioTask::TextToSpeech, AudioInput::Text(text)) => {
                limits.check_input_bytes(text.len() as u64)?;
                let session = self
                    .acquire_session()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                let mut session = session
                    .lock()
                    .map_err(|_| anyhow::anyhow!("TTS session mutex poisoned"))?;
                let output =
                    crate::audio::tts::tts_router::route_tts_text(self, &mut session, text)?;
                limits.check_decoded_bytes(output.len() as u64)?;
                Ok(AudioOutput::WavDataUri(output))
            }
            (AudioTask::SpeechToText | AudioTask::SpeechTranslation, AudioInput::Pcm(audio)) => {
                Ok(AudioOutput::Transcript(
                    self.execute_stt_prepared(audio, options, limits, callback)?,
                ))
            }
            _ => bail!("Audio task/input modality mismatch"),
        }
    }
}

pub(crate) fn is_uncached_decoder(session: &Session) -> bool {
    session.inputs().len() == 2
        && session.inputs().iter().any(|i| i.name() == "input_ids")
        && session
            .inputs()
            .iter()
            .any(|i| i.name() == "encoder_hidden_states")
        && session.outputs().iter().any(|o| o.name() == "logits")
}

pub(crate) fn validate_tensor(
    dtype: &ValueType,
    element: TensorElementType,
    expected: &[i64],
) -> Result<()> {
    let ValueType::Tensor { ty, shape, .. } = dtype else {
        bail!("Expected tensor graph input/output");
    };
    if *ty != element
        || shape
            .iter()
            .any(|&dimension| dimension == 0 || dimension < -1)
        || shape.len() != expected.len()
        || shape
            .iter()
            .zip(expected)
            .any(|(&a, &b)| a > 0 && b > 0 && a != b)
    {
        bail!("Tensor contract mismatch: {dtype:?}, expected {element:?} {expected:?}");
    }
    Ok(())
}

/// Processor dimensions are authoritative; model max_source_positions is a
/// post-convolution encoder dimension and must not replace mel frame count.
pub(crate) fn stt_config(engine: &OnnxEngine) -> Result<crate::audio::config::AudioConfig> {
    let dir = engine
        .model_dir
        .as_ref()
        .context("STT model directory missing")?;
    let processor: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("preprocessor_config.json"))?)?;
    let field = |name: &str| -> Result<usize> {
        processor
            .get(name)
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .with_context(|| format!("STT processor requires positive {name}"))
    };
    let mut config = crate::audio::config::AudioConfig::from_model_dir(&engine.model_dir);
    config.n_mels = field("feature_size")?;
    config.sample_rate = u32::try_from(field("sampling_rate")?)?;
    config.hop_length = field("hop_length")?;
    config.n_fft = field("n_fft")?;
    config.max_samples = field("n_samples")?;
    config.max_frames = field("nb_max_frames")?;
    if config.n_fft < 2
        || config.n_fft > config.max_samples
        || !config.n_fft.is_multiple_of(2)
        || config.n_mels > config.n_fft / 2 + 1
        || config.max_samples.checked_div(config.hop_length) != Some(config.max_frames)
    {
        bail!("Inconsistent STT processor dimensions");
    }
    let tokenizer = engine.tokenizer.as_ref().context("STT tokenizer missing")?;
    let token = |name: &str| {
        tokenizer
            .token_to_id(name)
            .map(i64::from)
            .with_context(|| format!("STT tokenizer missing {name}"))
    };
    config.start_of_transcript = token("<|startoftranscript|>")?;
    config.transcribe_token = token("<|transcribe|>")?;
    config.translate_token = token("<|translate|>")?;
    config.no_timestamps_token = token("<|notimestamps|>")?;
    config.end_of_text_token = token("<|endoftext|>")?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_task_contract_rejects_unimplemented_audio_tasks() -> Result<()> {
        assert_eq!(AudioTask::parse("text-to-speech")?, AudioTask::TextToSpeech);
        assert_eq!(
            AudioTask::parse("automatic-speech-recognition")?,
            AudioTask::SpeechToText
        );
        assert!(AudioTask::parse("music_generation").is_err());
        assert!(AudioTask::parse("speaker_diarization").is_err());
        Ok(())
    }
    #[test]
    fn test_tensor_contract_rejects_wrong_dtype_and_static_shape() -> ort::Result<()> {
        let tensor = ort::value::Tensor::from_array(([1usize, 2usize, 3usize], vec![0.0f32; 6]))?;
        assert!(validate_tensor(tensor.dtype(), TensorElementType::Float32, &[1, 2, 3]).is_ok());
        assert!(validate_tensor(tensor.dtype(), TensorElementType::Int64, &[1, 2, 3]).is_err());
        assert!(
            validate_tensor(tensor.dtype(), TensorElementType::Float32, &[1, 128, 3000]).is_err()
        );
        Ok(())
    }
}
