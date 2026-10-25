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
    pub temperature: Option<f32>,
    pub language: Option<String>,
    /// The speaker's languages (ISO 639-1): without `language`, speech is detected among only these
    #[serde(default)]
    pub languages: Vec<String>,
    pub speed: Option<f32>,
    pub voice_id: Option<String>,
    pub translate_to: Option<String>,
    pub timestamps: Option<bool>,
    pub beam_size: Option<u32>,
    pub vad_filter: Option<bool>,
    pub speaker_labels: Option<bool>,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AudioExecuteRequest {
    pub model: Option<String>,
    pub task: Option<String>,
    pub instruction: Option<String>,
    #[serde(deserialize_with = "strict_input_source")]
    #[schemars(with = "StrictInputSource")]
    pub input_source: InputSource,
    pub parameters: Option<AudioParameters>,
    pub keep_alive: Option<i32>,
}

// The standard transcription adapter keeps InputSource's existing permissive decoding.
// CEP's custom execution request rejects ignored nested fields and publishes that schema.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictInputSource {
    #[serde(rename = "type")]
    source_type: String,
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
