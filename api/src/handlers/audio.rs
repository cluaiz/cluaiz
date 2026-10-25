use crate::{state::AppState, utils::prepare_authorized_audio};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Json,
};
use cluaiz_models::types::manifest::{ModelRegistry, ModelRegistryEntry};
use cluaiz_runtime::onnx_host::{
    AudioEvent, AudioInput, AudioOutput as NativeOutput, AudioTask, OnnxModel, SttOptions,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};

#[path = "audio_types.rs"]
mod types;
pub use types::{
    AudioExecuteRequest, AudioExecuteResponse, AudioOutput, AudioParameters, AudioSegment,
    InputSource,
};

use crate::utils::task_error as http_error;

pub(crate) struct DispatchedAudio {
    pub events: tokio::sync::mpsc::Receiver<Result<AudioEvent, String>>,
    pub task: AudioTask,
    pub model: String,
}

pub(crate) enum AudioError {
    Http(Response, &'static str),
    Policy(cluaiz_cep::CepError),
}

impl AudioError {
    pub async fn into_cep(self) -> cluaiz_cep::CepError {
        match self {
            Self::Http(response, kind) => {
                let mut error = crate::cep::error_from_response(response).await;
                error.data.get_or_insert_with(|| json!({}))["type"] = json!(kind);
                error
            }
            Self::Policy(error) => error,
        }
    }

    fn into_standard(self) -> Response {
        match self {
            Self::Http(response, _) => response,
            // Standard routes pass no CEP Call; keep a defensive failure for policy errors.
            Self::Policy(error) => http_error(StatusCode::FORBIDDEN, "permission_error", error),
        }
    }
}

fn error<T>(
    status: StatusCode,
    kind: &'static str,
    message: impl ToString,
) -> Result<T, AudioError> {
    Err(AudioError::Http(http_error(status, kind, message), kind))
}

pub(crate) async fn selected_model(
    state: &Arc<AppState>,
    selected: Option<&str>,
    task: AudioTask,
) -> Result<ModelRegistryEntry, AudioError> {
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let schema = state.permission_schema.read().await;
    let slot = schema
        .active_slots
        .get(task.slot())
        .or_else(|| schema.active_slots.get("audio_slot"))
        .and_then(|slot| slot.model_id.as_deref());
    match select_model(&registry, selected, slot, task) {
        Ok(entry) => Ok(entry.clone()),
        Err(message) => error(StatusCode::NOT_FOUND, "model_not_found", message),
    }
}

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
        e.metadata
            .tts_family
            .as_deref()
            .is_some_and(cluaiz_runtime::onnx_host::is_experimental_family)
    };
    candidates.sort_by(|a, b| {
        experimental(a)
            .cmp(&experimental(b))
            .then_with(|| a.id.cmp(&b.id))
    });
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
            || params
                .voice_id
                .as_ref()
                .is_some_and(|v| !is_default_voice(v)))
    {
        anyhow::bail!("voice and speed apply to text_to_speech only");
    }
    if params
        .speed
        .is_some_and(|v| !(SPEED_RANGE.0..=SPEED_RANGE.1).contains(&v))
    {
        anyhow::bail!(
            "speed must be between {} and {}",
            SPEED_RANGE.0,
            SPEED_RANGE.1
        );
    }
    if task == AudioTask::TextToSpeech
        && (params.translate_to.is_some() || !params.languages.is_empty())
    {
        anyhow::bail!("languages and translate_to apply to speech-to-text only");
    }
    // TTS `language`: the text's language (Cluaiz extension, like Google/Azure languageCode)
    if params.language.as_deref().is_some_and(|code| {
        !(2..=12).contains(&code.len())
            || !code
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_')
    }) {
        anyhow::bail!("language must be a language code such as \"cs\", \"hi\" or \"en-US\"");
    }
    if params.languages.len() > MAX_SPEAKER_LANGUAGES
        || params.languages.iter().any(|code| {
            !(2..=5).contains(&code.len())
                || !code
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_')
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

pub(crate) fn audio_output(output: NativeOutput) -> AudioOutput {
    let (text, audio_data) = match output {
        NativeOutput::Transcript(text) => (Some(text), None),
        NativeOutput::WavDataUri(wav) => (None, Some(wav)),
    };
    AudioOutput {
        text,
        audio_data,
        segments: None,
    }
}

fn completed(model: String, task: AudioTask, output: NativeOutput) -> AudioExecuteResponse {
    AudioExecuteResponse {
        status: "success".into(),
        task: task.as_str().into(),
        model,
        info: None,
        output: audio_output(output),
    }
}

/// One dispatch/preparation boundary for standard routes and CEP; no chat dispatch.
pub(crate) async fn dispatch_audio(
    state: &Arc<AppState>,
    headers: HeaderMap,
    payload: AudioExecuteRequest,
    call: Option<&cluaiz_cep::Call>,
    cancellation: Arc<std::sync::atomic::AtomicBool>,
) -> Result<DispatchedAudio, AudioError> {
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
    let entry = selected_model(state, payload.model.as_deref(), task).await?;
    if let Some(call) = call {
        authorize_model(state, call, &entry)
            .await
            .map_err(AudioError::Policy)?;
    }
    let model_id = entry.id.clone();
    let settings = cluaiz_core::Settings::current();
    let admission = if task == AudioTask::TextToSpeech {
        match state.host.speech.try_acquire() {
            Ok(lease) => Some(lease),
            Err(e) => return error(StatusCode::TOO_MANY_REQUESTS, "speech_busy", e),
        }
    } else {
        None
    };
    if entry.format_type.eq_ignore_ascii_case("gguf") {
        if task != AudioTask::TextToSpeech {
            return error(
                StatusCode::BAD_REQUEST,
                "model_contract_error",
                "A GGUF speech model makes speech; speech-to-text needs an ONNX model",
            );
        }
        if let Some(language) = options
            .language
            .as_deref()
            .filter(|l| !l.eq_ignore_ascii_case("auto"))
        {
            let caps = match cluaiz_runtime::speech_host::capabilities(&entry) {
                Ok(caps) => caps,
                Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
            };
            if let Err(e) = caps.validate_language_control(Some(language)) {
                return error(StatusCode::BAD_REQUEST, "unsupported_parameter", e);
            }
            let base = language
                .split(['-', '_'])
                .next()
                .unwrap_or(language)
                .to_ascii_lowercase();
            if !caps.languages.is_empty() && !caps.languages.contains(&base) {
                return error(StatusCode::BAD_REQUEST, "unsupported_language", format!("Selected speech model does not support language '{language}'. Supported languages: {}", caps.languages.join(", ")));
            }
        }
        let Some(admission) = admission else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "speech_admission_error",
                "Missing speech admission",
            );
        };
        let speaker_reference = if let Some(mut source) = options.voice {
            if let Some(call) = call {
                crate::utils::authorize_source_for_call(state, call, &mut source)
                    .await
                    .map_err(AudioError::Policy)?;
            }
            let (owner, request_headers, request_settings) =
                (Arc::clone(state), headers.clone(), settings.clone());
            let source_cancel = Arc::clone(&cancellation);
            let preparation = admission
                .clone()
                .prepare(Arc::clone(&cancellation), async move {
                    prepare_authorized_audio(
                        &owner,
                        &request_headers,
                        source,
                        &request_settings,
                        Some(source_cancel),
                    )
                    .await
                })
                .await;
            let prepared = match preparation {
                Ok((result, _lease)) => result,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, "audio_decode_error", e),
            };
            match prepared {
                Ok(audio) => Some(audio),
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
        } else {
            None
        };
        let speech_options = cluaiz_runtime::speech_host::SpeechOptions {
            language: options.language,
            speed: options.speed,
            speaker_reference,
            limits: settings.media.limits,
            ..Default::default()
        };
        let onnx = state.host.onnx.clone();
        let release_lease = admission.clone();
        match tokio::task::spawn_blocking(move || {
            let _lease = release_lease;
            onnx.unload_tts()
        })
        .await
        {
            Ok(Ok(_)) => {}
            result => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "speech_transition_error",
                    format!("{result:?}"),
                )
            }
        }
        let events = state.host.speech.dispatch(
            entry,
            payload.input_source.data,
            speech_options,
            settings,
            payload.keep_alive,
            admission,
        );
        return Ok(DispatchedAudio {
            events,
            task,
            model: model_id,
        });
    }
    let model = match OnnxModel::from_registry(&entry, task) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "model_contract_error", e),
    };
    if task == AudioTask::TextToSpeech {
        match model
            .tts_capabilities()
            .and_then(|caps| caps.validate_language_control(options.language.as_deref()))
        {
            Ok(()) => {}
            Err(e) => return error(StatusCode::BAD_REQUEST, "unsupported_parameter", e),
        }
    }
    let input = if is_text {
        AudioInput::Text(payload.input_source.data)
    } else {
        let mut source = match payload
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
        if let Some(call) = call {
            crate::utils::authorize_source_for_call(state, call, &mut source)
                .await
                .map_err(AudioError::Policy)?;
        }
        match prepare_authorized_audio(state, &headers, source, &settings, Some(cancellation)).await
        {
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
        match tokio::task::spawn_blocking(move || {
            let _lease = release_lease;
            speech.unload()
        })
        .await
        {
            Ok(Ok(())) => {}
            result => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "speech_transition_error",
                    format!("{result:?}"),
                )
            }
        }
    }
    let events = state.host.onnx.dispatch(
        model,
        task,
        input,
        options,
        settings,
        payload.keep_alive,
        admission,
    );
    Ok(DispatchedAudio {
        events,
        task,
        model: model_id,
    })
}

pub(crate) async fn authorize_model(
    state: &Arc<AppState>,
    call: &cluaiz_cep::Call,
    entry: &ModelRegistryEntry,
) -> Result<(), cluaiz_cep::CepError> {
    let path = entry.primary_path().ok_or_else(|| {
        cluaiz_cep::CepError::not_found("model_not_found", "Audio model weight is missing")
    })?;
    let mut source = path.to_string_lossy().to_string();
    crate::utils::authorize_source_for_call(state, call, &mut source).await
}

async fn standard_audio(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: AudioExecuteRequest,
    stream: bool,
) -> Response {
    let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(false));
    match dispatch_audio(&state, headers, request, None, cancellation).await {
        Ok(ready) => respond(ready.events, ready.task, ready.model, stream).await,
        Err(error) => error.into_standard(),
    }
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
                return http_error(StatusCode::INTERNAL_SERVER_ERROR, "engine_error", message)
            }
        }
    }
    http_error(
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
        Err(e) => return http_error(StatusCode::BAD_REQUEST, "invalid_input", e),
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
        keep_alive: payload.keep_alive,
    };
    standard_audio(state, headers, request, payload.stream.unwrap_or(false)).await
}

pub async fn transcriptions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let payload: TranscriptionRequest = match serde_json::from_value(payload) {
        Ok(request) => request,
        Err(e) => return http_error(StatusCode::BAD_REQUEST, "invalid_input", e),
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
        keep_alive: payload.keep_alive,
    };
    standard_audio(state, headers, request, payload.stream.unwrap_or(false)).await
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
