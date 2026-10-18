use crate::{state::AppState, utils::prepare_authorized_media};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Json,
};
use cluaiz_core::{
    media::{InputPart, PreparedPart},
    ChatEnvelope, ChatMessage,
};
use cluaiz_models::types::manifest::{ModelRegistry, ModelRegistryEntry};
use cluaiz_runtime::onnx_host::{
    AudioEvent, AudioInput, AudioOutput as NativeOutput, AudioTask, OnnxModel, SttOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};

#[derive(Deserialize, Debug, Clone)]
pub struct InputSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub data: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AudioParameters {
    pub temperature: Option<f32>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub voice_id: Option<String>,
    pub translate_to: Option<String>,
    pub timestamps: Option<bool>,
    pub beam_size: Option<u32>,
    pub vad_filter: Option<bool>,
    pub speaker_labels: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AudioExecuteRequest {
    pub model: Option<String>,
    pub task: Option<String>,
    pub instruction: Option<String>,
    pub input_source: InputSource,
    pub parameters: Option<AudioParameters>,
    pub stream: Option<bool>,
    pub keep_alive: Option<i32>,
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

use crate::utils::task_error as error;

fn select_model<'a>(
    registry: &'a ModelRegistry,
    selected: Option<&str>,
    slot: Option<&str>,
    task: AudioTask,
) -> anyhow::Result<&'a ModelRegistryEntry> {
    let supports = |entry: &&ModelRegistryEntry| {
        entry.format_type.eq_ignore_ascii_case("onnx")
            && entry
                .supported_tasks
                .iter()
                .any(|t| AudioTask::parse(t).ok() == Some(task))
    };
    if let Some(id) = selected.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto")) {
        return registry
            .installed_models
            .get(id)
            .or_else(|| {
                registry
                    .installed_models
                    .values()
                    .find(|e| e.id.eq_ignore_ascii_case(id))
            })
            .ok_or_else(|| anyhow::anyhow!("Requested audio model is not installed: {id}"));
    }
    if let Some(entry) = slot
        .and_then(|id| registry.installed_models.get(id))
        .filter(supports)
    {
        return Ok(entry);
    }
    let mut candidates: Vec<_> = registry
        .installed_models
        .values()
        .filter(supports)
        .collect();
    // Automatic choice: working families first (an unfinished one only when nothing else is installed)
    let experimental = |e: &ModelRegistryEntry| {
        e.metadata.tts_family.as_deref().is_some_and(cluaiz_runtime::onnx_host::is_experimental_family)
    };
    candidates.sort_by(|a, b| experimental(a).cmp(&experimental(b)).then_with(|| a.id.cmp(&b.id)));
    candidates
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("No installed ONNX model declares {}", task.as_str()))
}

fn validate_options(payload: &AudioExecuteRequest, task: AudioTask) -> anyhow::Result<SttOptions> {
    if payload.keep_alive.is_some_and(|v| v < -1) {
        anyhow::bail!("keep_alive must be -1, 0, or positive minutes");
    }
    if payload
        .instruction
        .as_ref()
        .is_some_and(|v| !v.trim().is_empty())
    {
        anyhow::bail!("Audio instruction prompts are not implemented for this graph contract");
    }
    let params = payload.parameters.clone().unwrap_or_default();
    if params.timestamps == Some(true)
        || params.vad_filter == Some(true)
        || params.speaker_labels == Some(true)
        || params.beam_size.is_some_and(|v| v != 1)
        || params.temperature.is_some_and(|v| v != 0.0)
    {
        anyhow::bail!("Requested audio option is not implemented: greedy STT without timestamps/VAD/diarization");
    }
    // TTS voice and rate: "default"/"auto"/empty lets the engine pick a voice for the text's language
    if task != AudioTask::TextToSpeech
        && (params.speed.is_some_and(|v| v != 1.0)
            || params.voice_id.as_ref().is_some_and(|v| !is_default_voice(v)))
    {
        anyhow::bail!("voice and speed apply to text_to_speech only");
    }
    if params.speed.is_some_and(|v| !(SPEED_RANGE.0..=SPEED_RANGE.1).contains(&v)) {
        anyhow::bail!("speed must be between {} and {}", SPEED_RANGE.0, SPEED_RANGE.1);
    }
    if task == AudioTask::TextToSpeech
        && (params.language.is_some() || params.translate_to.is_some())
    {
        anyhow::bail!("Language/translation options are not implemented for TTS");
    }
    if let Some(target) = params
        .translate_to
        .as_deref()
        .filter(|v| !v.is_empty() && *v != "false")
    {
        if task != AudioTask::SpeechTranslation || target != "en" {
            anyhow::bail!("Speech translation currently supports only the declared speech_translation task with English output");
        }
    }
    Ok(SttOptions {
        language: params.language,
        translate: task == AudioTask::SpeechTranslation,
        voice: params.voice_id.filter(|v| !is_default_voice(v)),
        speed: params.speed,
    })
}

/// Speaking rates accepted for text_to_speech (OpenAI's /v1/audio/speech range)
const SPEED_RANGE: (f32, f32) = (0.25, 4.0);

fn is_default_voice(voice: &str) -> bool {
    let voice = voice.trim();
    voice.is_empty() || voice.eq_ignore_ascii_case("default") || voice.eq_ignore_ascii_case("auto")
}

fn completed(model: String, task: AudioTask, output: NativeOutput) -> AudioExecuteResponse {
    let (text, audio_data) = match output {
        NativeOutput::Transcript(text) => (Some(text), None),
        NativeOutput::WavDataUri(wav) => (None, Some(wav)),
    };
    AudioExecuteResponse {
        status: "success".into(),
        task: task.as_str().into(),
        model,
        info: None,
        output: AudioOutput {
            text,
            audio_data,
            segments: None,
        },
    }
}

/// Existing audio routes share one typed ONNX boundary; no chat dispatch occurs here.
pub async fn execute_audio(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<AudioExecuteRequest>,
) -> Response {
    let is_text = payload
        .input_source
        .source_type
        .eq_ignore_ascii_case("text");
    let task = match payload
        .task
        .as_deref()
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
    {
        Some(value) => match AudioTask::parse(value) {
            Ok(task) => task,
            Err(e) => return error(StatusCode::BAD_REQUEST, "unsupported_task", e),
        },
        None => {
            if is_text {
                AudioTask::TextToSpeech
            } else {
                AudioTask::SpeechToText
            }
        }
    };
    if payload.input_source.data.trim().is_empty() || is_text != (task == AudioTask::TextToSpeech) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Empty input or audio task/input modality mismatch",
        );
    }
    let options = match validate_options(&payload, task) {
        Ok(options) => options,
        Err(e) => return error(StatusCode::BAD_REQUEST, "unsupported_parameter", e),
    };
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let schema = state.permission_schema.read().await;
    let slot_id = schema
        .active_slots
        .get(task.slot())
        .or_else(|| schema.active_slots.get("audio_slot"))
        .and_then(|s| s.model_id.as_deref());
    let entry = match select_model(&registry, payload.model.as_deref(), slot_id, task) {
        Ok(entry) => entry,
        Err(e) => return error(StatusCode::NOT_FOUND, "model_not_found", e),
    };
    drop(schema);
    let model_id = entry.id.clone();
    let model = match OnnxModel::from_registry(entry, task) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    };
    let settings = cluaiz_core::Settings::current();
    let input = if is_text {
        AudioInput::Text(payload.input_source.data)
    } else {
        let source = match payload
            .input_source
            .source_type
            .to_ascii_lowercase()
            .as_str()
        {
            "url" | "file" | "audio" => payload.input_source.data,
            "base64" => {
                if payload.input_source.data.starts_with("data:") {
                    payload.input_source.data
                } else {
                    format!(
                        "data:application/octet-stream;base64,{}",
                        payload.input_source.data
                    )
                }
            }
            _ => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Unsupported audio source type",
                )
            }
        };
        let request = ChatEnvelope {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: vec![InputPart::Audio { source }],
            }],
            samplers: Default::default(),
            think_mode: "off".into(),
            reasoning_effort: Default::default(),
            reasoning_budget_message: None,
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
        };
        let prepared =
            match prepare_authorized_media(&state, &headers, request, &settings, None, None).await {
                Ok(request) => request,
                Err((kind, message)) => {
                    return error(
                        if kind == "permission_error" {
                            StatusCode::FORBIDDEN
                        } else {
                            StatusCode::BAD_REQUEST
                        },
                        kind,
                        message,
                    )
                }
            };
        match prepared.messages.into_iter().flat_map(|m| m.content).next() {
            Some(PreparedPart::Audio { audio }) => AudioInput::Pcm(audio),
            _ => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Audio source did not produce PCM",
                )
            }
        }
    };
    let mut events =
        state
            .host
            .onnx
            .dispatch(model, task, input, options, settings, payload.keep_alive);
    if payload.stream.unwrap_or(true) {
        let stream = async_stream::stream! {
            let mut terminal_seen = false;
            let mut audio_index = 0_u64;
            while let Some(event) = events.recv().await {
                let body = match event {
                    // Speech: each finished phrase as its own WAV, playable in order while the rest is made
                    Ok(AudioEvent::Chunk(audio)) if task == AudioTask::TextToSpeech => {
                        audio_index += 1;
                        json!({"status":"chunk","type":"speech.audio.delta","index":audio_index - 1,"format":"wav","audio":audio})
                    }
                    Ok(AudioEvent::Chunk(token)) => json!({"status":"chunk","token":token}),
                    Ok(AudioEvent::Completed(output)) => json!({"status":"done","result":completed(model_id.clone(),task,output)}),
                    Err(message) => json!({"status":"error","error":{"type":"engine_error","message":message}}),
                };
                let terminal = body["status"] != "chunk";
                yield Ok::<_,Infallible>(Event::default().data(body.to_string()));
                if terminal { terminal_seen = true; break; }
            }
            if !terminal_seen {
                yield Ok::<_,Infallible>(Event::default().data(json!({"status":"error",
                    "error":{"type":"engine_error","message":"Audio worker ended without a result"}}).to_string()));
            }
        };
        return Sse::new(stream).into_response();
    }
    while let Some(event) = events.recv().await {
        match event {
            Ok(AudioEvent::Chunk(_)) => {}
            Ok(AudioEvent::Completed(output)) => {
                return Json(completed(model_id, task, output)).into_response()
            }
            Err(message) => {
                return error(StatusCode::INTERNAL_SERVER_ERROR, "engine_error", message)
            }
        }
    }
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "engine_error",
        "Audio worker ended without a result",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeechRequest {
    model: Option<String>,
    input: String,
    voice: Option<String>,
    speed: Option<f32>,
    stream: Option<bool>,
    keep_alive: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptionRequest {
    model: Option<String>,
    file: Option<String>,
    input_source: Option<InputSource>,
    prompt: Option<String>,
    language: Option<String>,
    temperature: Option<f32>,
    timestamps: Option<bool>,
    stream: Option<bool>,
    keep_alive: Option<i32>,
}

pub async fn speech(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let payload: SpeechRequest = match serde_json::from_value(payload) {
        Ok(request) => request,
        Err(e) => return error(StatusCode::BAD_REQUEST, "invalid_input", e),
    };
    let request = AudioExecuteRequest {
        model: payload.model,
        task: Some("text_to_speech".into()),
        instruction: None,
        input_source: InputSource {
            source_type: "text".into(),
            data: payload.input,
        },
        parameters: Some(AudioParameters {
            speed: payload.speed,
            voice_id: payload.voice,
            ..Default::default()
        }),
        stream: payload.stream.or(Some(false)),
        keep_alive: payload.keep_alive,
    };
    execute_audio(State(state), headers, Json(request)).await
}

#[derive(Deserialize)]
pub struct VoicesQuery {
    model: Option<String>,
}

/// GET /v1/audio/voices?model=<id>: the voices a TTS model can speak with. "default" (the engine
/// picks a voice for the text's language) comes first; a single-voice model has only it.
pub async fn voices(State(state): State<Arc<AppState>>, Query(query): Query<VoicesQuery>) -> Response {
    let task = AudioTask::TextToSpeech;
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let schema = state.permission_schema.read().await;
    let slot_id = schema
        .active_slots
        .get(task.slot())
        .or_else(|| schema.active_slots.get("audio_slot"))
        .and_then(|s| s.model_id.as_deref());
    let entry = match select_model(&registry, query.model.as_deref(), slot_id, task) {
        Ok(entry) => entry,
        Err(e) => return error(StatusCode::NOT_FOUND, "model_not_found", e),
    };
    drop(schema);
    let model = match OnnxModel::from_registry(entry, task) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    };
    let mut data = vec![json!({"id": "default", "language": null, "default": true})];
    data.extend(
        model
            .voices()
            .into_iter()
            .map(|v| json!({"id": v.id, "language": v.language, "default": false})),
    );
    Json(json!({"object": "list", "model": model.id, "data": data})).into_response()
}

pub async fn transcriptions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let payload: TranscriptionRequest = match serde_json::from_value(payload) {
        Ok(request) => request,
        Err(e) => return error(StatusCode::BAD_REQUEST, "invalid_input", e),
    };
    let input_source = payload.input_source.unwrap_or(InputSource {
        source_type: "file".into(),
        data: payload.file.unwrap_or_default(),
    });
    let request = AudioExecuteRequest {
        model: payload.model,
        task: Some("speech_to_text".into()),
        instruction: payload.prompt,
        input_source,
        parameters: Some(AudioParameters {
            language: payload.language,
            temperature: payload.temperature,
            timestamps: payload.timestamps,
            ..Default::default()
        }),
        stream: payload.stream.or(Some(false)),
        keep_alive: payload.keep_alive,
    };
    execute_audio(State(state), headers, Json(request)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> AudioExecuteRequest {
        AudioExecuteRequest {
            model: None,
            task: None,
            instruction: None,
            input_source: InputSource {
                source_type: "text".into(),
                data: "sample".into(),
            },
            parameters: None,
            stream: None,
            keep_alive: None,
        }
    }
    #[test]
    fn test_unsupported_audio_options_are_errors() {
        let mut request = request();
        assert!(validate_options(&request, AudioTask::TextToSpeech).is_ok());
        request.parameters = Some(AudioParameters {
            voice_id: Some("hf_alpha".into()),
            speed: Some(1.5),
            ..Default::default()
        });
        let options = validate_options(&request, AudioTask::TextToSpeech).unwrap();
        assert_eq!((options.voice.as_deref(), options.speed), (Some("hf_alpha"), Some(1.5)));
        assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
        request.parameters = Some(AudioParameters {
            voice_id: Some("default".into()),
            ..Default::default()
        });
        assert!(validate_options(&request, AudioTask::TextToSpeech).unwrap().voice.is_none());
        request.parameters = Some(AudioParameters {
            speed: Some(9.0),
            ..Default::default()
        });
        assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
        request.parameters = Some(AudioParameters {
            timestamps: Some(true),
            ..Default::default()
        });
        assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
        request.parameters = None;
        request.keep_alive = Some(-2);
        assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
    }
    #[test]
    fn test_explicit_missing_model_is_not_replaced() {
        let entry:ModelRegistryEntry=serde_json::from_value(json!({
            "id":"model-long","category":"tts","format_type":"ONNX","huggingface_repo":"",
            "local_dir":"","files":[],"extra_files":null,"supported_tasks":["text-to-speech"],"requires_gpu":false,
            "metadata":{"architecture":"test","parameters":"","context_window":"",
            "quantization":null,"bit_depth":null,"chat_template":null}
        })).expect("Test registry fixture");
        let mut registry = ModelRegistry::default();
        registry.installed_models.insert(entry.id.clone(), entry);
        assert_eq!(
            select_model(&registry, None, None, AudioTask::TextToSpeech)
                .expect("Declared TTS model")
                .id,
            "model-long"
        );
        assert!(select_model(
            &registry,
            Some("model"),
            Some("other"),
            AudioTask::TextToSpeech
        )
        .is_err());
    }
}
