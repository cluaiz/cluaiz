//! Shared request/output records; standard adapters retain their separate input shapes.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema, Debug, Clone)]
pub struct InputSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub data: String,
}

#[derive(Deserialize, JsonSchema, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AudioParameters {
    /// Transcription sampling temperature; only 0 (greedy) is supported
    pub temperature: Option<f32>,
    /// Speech: the text's language ("cs", "hi", "en-US"). Transcription: the spoken language, or "auto"
    pub language: Option<String>,
    /// The speaker's languages (ISO 639-1): without `language`, speech is detected among only these
    #[serde(default)]
    pub languages: Vec<String>,
    /// Speech rate, 0.25 to 4.0 (speech only)
    pub speed: Option<f32>,
    /// A voice id from `audio.voices`; "default" / "auto" picks one for the text's language (speech only)
    pub voice_id: Option<String>,
    /// "en" with a model that declares speech_translation; other targets are refused
    pub translate_to: Option<String>,
    /// Only false is supported
    pub timestamps: Option<bool>,
    /// Only 1 is supported
    pub beam_size: Option<u32>,
    /// Only false is supported
    pub vad_filter: Option<bool>,
    /// Only false is supported
    pub speaker_labels: Option<bool>,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AudioExecuteRequest {
    /// An installed speech or transcription model id (default: the active slot, else the first installed model for the task)
    pub model: Option<String>,
    /// text_to_speech, speech_to_text or speech_translation (default: from `input_source.type`, text = speech)
    pub task: Option<String>,
    /// Not supported: a non-empty value is refused
    pub instruction: Option<String>,
    /// Input descriptor: text for speech generation, or a local/remote/inline audio source for transcription
    #[serde(deserialize_with = "strict_input_source")]
    #[schemars(with = "StrictInputSource")]
    pub input_source: InputSource,
    /// Task options
    pub parameters: Option<AudioParameters>,
    /// Minutes the model stays loaded after the task: -1 = until replaced, 0 = unload now, N = N minutes
    pub keep_alive: Option<i32>,
}

// The standard transcription adapter keeps InputSource's existing permissive decoding.
// CEP's custom execution request rejects ignored nested fields and publishes that schema.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictInputSource {
    /// text (speech), or file / url / audio / base64 (transcription)
    #[serde(rename = "type")]
    source_type: String,
    /// The text; a file path; an http(s) URL; or a `data:audio/...;base64,` URI
    data: String,
}

fn strict_input_source<'de, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<InputSource, D::Error> {
    let source = StrictInputSource::deserialize(input)?;
    Ok(InputSource {
        source_type: source.source_type,
        data: source.data,
    })
}

#[derive(Serialize, Debug, Clone)]
pub struct AudioSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
    pub speaker: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AudioOutput {
    pub text: Option<String>,
    pub audio_data: Option<String>,
    pub segments: Option<Vec<AudioSegment>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AudioExecuteResponse {
    pub status: String,
    pub task: String,
    pub model: String,
    pub info: Option<String>,
    pub output: AudioOutput,
}
