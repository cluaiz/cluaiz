use crate::{state::AppState, utils::prepare_authorized_audio};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Json,
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
    // ONNX audio packages, and GGUF speech models (llama.cpp audio generation) for text-to-speech
    let supports = |entry: &&ModelRegistryEntry| {
        (entry.format_type.eq_ignore_ascii_case("onnx")
            || (task == AudioTask::TextToSpeech && entry.format_type.eq_ignore_ascii_case("gguf")))
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
    if task == AudioTask::TextToSpeech && (params.translate_to.is_some() || !params.languages.is_empty()) {
        anyhow::bail!("languages and translate_to apply to speech-to-text only");
    }
    // TTS `language`: the text's language (Cluaiz extension, like Google/Azure languageCode)
    if params.language.as_deref().is_some_and(|code| {
        !(2..=12).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_')
    }) {
        anyhow::bail!("language must be a language code such as \"cs\", \"hi\" or \"en-US\"");
    }
    if params.languages.len() > MAX_SPEAKER_LANGUAGES
        || params.languages.iter().any(|code| {
            !(2..=5).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_')
        })
    {
        anyhow::bail!("languages must be at most {MAX_SPEAKER_LANGUAGES} language codes such as \"hi\" or \"en\"");
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
        languages: params.languages,
        translate: task == AudioTask::SpeechTranslation,
        voice: params.voice_id.filter(|v| !is_default_voice(v)),
        speed: params.speed,
    })
}

/// Speaking rates accepted for text_to_speech (OpenAI's /v1/audio/speech range)
const SPEED_RANGE: (f32, f32) = cluaiz_media::tempo::SPEED_RANGE;

/// A speaker's languages per request (a person rarely speaks more; detection gets less reliable)
const MAX_SPEAKER_LANGUAGES: usize = 10;

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
    let stream = payload.stream.unwrap_or(true);
    let settings = cluaiz_core::Settings::current();
    let admission = if task == AudioTask::TextToSpeech {
        match state.host.speech.try_acquire() {
            Ok(lease) => Some(lease),
            Err(e) => return error(StatusCode::TOO_MANY_REQUESTS, "speech_busy", e),
        }
    } else { None };
    if entry.format_type.eq_ignore_ascii_case("gguf") {
        if task != AudioTask::TextToSpeech {
            return error(StatusCode::BAD_REQUEST, "model_contract_error", "A GGUF speech model makes speech; speech-to-text needs an ONNX model");
        }
        if let Some(language) = options.language.as_deref().filter(|l| !l.eq_ignore_ascii_case("auto")) {
            let caps = match cluaiz_runtime::speech_host::capabilities(entry) {
                Ok(caps) => caps,
                Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
            };
            if let Err(e) = caps.validate_language_control(Some(language)) {
                return error(StatusCode::BAD_REQUEST, "unsupported_parameter", e);
            }
            let base = language.split(['-', '_']).next().unwrap_or(language).to_ascii_lowercase();
            if !caps.languages.is_empty() && !caps.languages.contains(&base) {
                return error(StatusCode::BAD_REQUEST, "unsupported_language", format!("Selected speech model does not support language '{language}'. Supported languages: {}", caps.languages.join(", ")));
            }
        }
        let Some(admission) = admission else {
            return error(StatusCode::INTERNAL_SERVER_ERROR, "speech_admission_error", "Missing speech admission");
        };
        let speaker_reference = if let Some(source) = options.voice {
            let (owner, request_headers, request_settings) = (Arc::clone(&state), headers.clone(), settings.clone());
            let preparation = admission.clone().prepare(Arc::new(std::sync::atomic::AtomicBool::new(false)), async move {
                prepare_authorized_audio(&owner, &request_headers, source, &request_settings).await
            }).await;
            let prepared = match preparation {
                Ok((result, _lease)) => result,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, "audio_decode_error", e),
            };
            match prepared {
                Ok(audio) => Some(audio),
                Err((kind, message)) => return error(if kind == "permission_error" {StatusCode::FORBIDDEN} else {StatusCode::BAD_REQUEST}, kind, message),
            }
        } else { None };
        let speech_options = cluaiz_runtime::speech_host::SpeechOptions {
            language: options.language,
            speed: options.speed,
            speaker_reference,
            limits: settings.media.limits,
            ..Default::default()
        };
        let onnx = state.host.onnx.clone();
        let release_lease = admission.clone();
        match tokio::task::spawn_blocking(move || { let _lease = release_lease; onnx.unload_tts() }).await {
            Ok(Ok(_)) => {},
            result => return error(StatusCode::INTERNAL_SERVER_ERROR, "speech_transition_error", format!("{result:?}")),
        }
        let events = state.host.speech.dispatch(entry.clone(), payload.input_source.data, speech_options, settings, payload.keep_alive, admission);
        return respond(events, task, model_id, stream).await;
    }
    let model = match OnnxModel::from_registry(entry, task) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    };
    if task == AudioTask::TextToSpeech {
        match model.tts_capabilities().and_then(|caps| caps.validate_language_control(options.language.as_deref())) {
            Ok(()) => {},
            Err(e) => return error(StatusCode::BAD_REQUEST, "unsupported_parameter", e),
        }
    }
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
        match prepare_authorized_audio(&state, &headers, source, &settings).await {
                Ok(audio) => AudioInput::Pcm(audio),
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
        }
    };
    if task == AudioTask::TextToSpeech {
        let speech = state.host.speech.clone();
        let release_lease = admission.clone();
        match tokio::task::spawn_blocking(move || { let _lease = release_lease; speech.unload() }).await {
            Ok(Ok(())) => {},
            result => return error(StatusCode::INTERNAL_SERVER_ERROR, "speech_transition_error", format!("{result:?}")),
        }
    }
    let events = state
        .host
        .onnx
        .dispatch(model, task, input, options, settings, payload.keep_alive, admission);
    respond(events, task, model_id, stream).await
}

/// The task's events as Server-Sent Events (`stream`) or one JSON result.
async fn respond(
    mut events: tokio::sync::mpsc::Receiver<Result<AudioEvent, String>>,
    task: AudioTask,
    model_id: String,
    stream: bool,
) -> Response {
    if stream {
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
    /// Cluaiz extension: the text's language, so text in a language no voice speaks is still read
    /// with that language's rules (default: from the text's script, the voice or the model)
    language: Option<String>,
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
    /// Cluaiz extension: the speaker's languages; each request is detected among only these
    #[serde(default)]
    languages: Vec<String>,
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
            language: payload.language,
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
    let mut data = vec![json!({"id": "default", "language": null, "default": true})];
    // A GGUF speech model has no voice files; a nondefault voice is a reference audio source.
    if entry.format_type.eq_ignore_ascii_case("gguf") {
        return match cluaiz_runtime::speech_host::capabilities(entry) {
            Ok(capabilities) => Json(json!({"object":"list", "model":entry.id, "data":data, "capabilities":capabilities})).into_response(),
            Err(e) => error(StatusCode::BAD_REQUEST, "model_contract_error", e),
        };
    }
    let model = match OnnxModel::from_registry(entry, task) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    };
    data.extend(
        model
            .voices()
            .into_iter()
            .map(|v| json!({"id": v.id, "language": v.language, "default": false})),
    );
    match model.tts_capabilities() {
        Ok(capabilities) => Json(json!({"object":"list", "model":model.id, "data":data, "capabilities":capabilities})).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    }
}

/// GET /v1/audio/languages?model=<id>: the languages a speech-to-text model can transcribe (codes
/// for `language` and `languages`). Empty for a model that reads a single language.
pub async fn languages(State(state): State<Arc<AppState>>, Query(query): Query<VoicesQuery>) -> Response {
    let task = AudioTask::SpeechToText;
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
    Json(json!({"object": "list", "model": model.id, "data": model.stt_languages()})).into_response()
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
            languages: payload.languages,
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
        // The text's language for TTS; the speaker's languages only for STT
        request.parameters = Some(AudioParameters {
            language: Some("cs".into()),
            ..Default::default()
        });
        assert_eq!(validate_options(&request, AudioTask::TextToSpeech).unwrap().language.as_deref(), Some("cs"));
        request.parameters = Some(AudioParameters {
            language: Some("not a code!".into()),
            ..Default::default()
        });
        assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
        request.parameters = Some(AudioParameters {
            languages: vec!["hi".into()],
            ..Default::default()
        });
        assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
        assert!(validate_options(&request, AudioTask::SpeechToText).is_ok());
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
