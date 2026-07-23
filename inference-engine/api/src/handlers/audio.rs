use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use crate::state::AppState;

#[derive(Deserialize, Debug, Clone)]
pub struct InputSource {
    #[serde(rename = "type")]
    pub source_type: String, // "url", "base64", "text"
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
    pub task: Option<String>, // "auto", "speech_to_text", "text_to_speech", etc.
    pub instruction: Option<String>,
    pub input_source: InputSource,
    pub parameters: Option<AudioParameters>,
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
    pub audio_data: Option<String>, // Base64 encoded audio for TTS/audio-to-audio
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

// ─── POST /v1/audio/execute ─────────────────────────────────────────
pub async fn execute_audio(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<AudioExecuteRequest>,
) -> impl IntoResponse {
    let registry = cluaiz_shared::utils::model_registry::ModelRegistry::load();

    // 1. Model Resolution & Validation (404 Error if Model Not Installed)
    let target_model_id = match payload.model.as_deref() {
        Some(m) if !m.trim().is_empty() && m != "auto" => {
            if !registry.installed_models.contains_key(m) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "error": format!("Requested audio model '{}' is not installed in ModelRegistry.", m),
                        "status": "error"
                    })),
                );
            }
            m.to_string()
        }
        _ => {
            let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            let mut resolved_id = String::new();
            if let Some(slot) = schema.active_slots.get("audio_slot") {
                if let Some(ref active_id) = slot.model_id {
                    if registry.installed_models.contains_key(active_id) {
                        resolved_id = active_id.clone();
                    }
                }
            }
            if resolved_id.is_empty() {
                resolved_id = registry
                    .installed_models
                    .values()
                    .find(|e| e.category == "audio")
                    .map(|e| e.id.clone())
                    .unwrap_or_default();
            }
            if resolved_id.is_empty() {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "error": "No installed audio models found in engine registry. Please install an audio model first.",
                        "status": "error"
                    })),
                );
            }
            resolved_id
        }
    };

    let target_entry = &registry.installed_models[&target_model_id];
    let supported_tasks = &target_entry.supported_tasks;
    let primary_task = supported_tasks
        .first()
        .cloned()
        .unwrap_or_else(|| "speech_to_text".to_string());

    // 2. Task Resolution (No Error on task mismatch -> Auto-bind + Informative Alert)
    let mut task_info = None;
    let resolved_task = match payload.task.as_deref() {
        Some(user_task) if !user_task.trim().is_empty() && user_task != "auto" => {
            let norm_user = user_task.to_lowercase().replace("-", "_");
            let norm_primary = primary_task.to_lowercase().replace("-", "_");
            if norm_user != norm_primary
                && !supported_tasks
                    .iter()
                    .any(|t| t.to_lowercase().replace("-", "_") == norm_user)
            {
                task_info = Some(format!(
                    "Model '{}' supports {:?}. Auto-aligned requested task '{}' to model's primary capability '{}'.",
                    target_model_id, supported_tasks, user_task, primary_task
                ));
                primary_task.clone()
            } else {
                norm_user
            }
        }
        _ => primary_task.clone(),
    };

    // 3. Modality Validation (Error ONLY when Model Capability & Input Data type mismatch!)
    let input_type = payload.input_source.source_type.to_lowercase();
    let is_text_input = input_type == "text";
    let is_audio_input =
        input_type == "url" || input_type == "base64" || input_type == "file" || input_type == "audio";

    let is_tts_model = resolved_task == "text_to_speech" || resolved_task == "music_generation";
    let is_stt_model = resolved_task == "speech_to_text"
        || resolved_task == "speech_translation"
        || resolved_task == "audio_classification"
        || resolved_task == "speaker_diarization";

    if is_tts_model && is_audio_input {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "Input modality mismatch: Model '{}' is configured for text-to-audio synthesis (task: '{}'), but an Audio file input ('{}') was provided. Expected Text input source.",
                    target_model_id, resolved_task, input_type
                ),
                "status": "error"
            })),
        );
    }

    if is_stt_model && is_text_input {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "Input modality mismatch: Model '{}' is configured for audio processing/transcription (task: '{}'), but a Text input source ('text') was provided. Expected an Audio file input source (url, base64, or file).",
                    target_model_id, resolved_task
                ),
                "status": "error"
            })),
        );
    }

    let response = AudioExecuteResponse {
        status: "success".to_string(),
        task: resolved_task,
        model: target_model_id,
        info: task_info,
        output: AudioOutput {
            text: Some(format!(
                "Audio execution processed for input type: {}",
                payload.input_source.source_type
            )),
            audio_data: None,
            segments: Some(vec![]),
        },
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(&response).unwrap()),
    )
}
