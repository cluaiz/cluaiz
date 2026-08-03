use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Sse, sse::Event},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::convert::Infallible;
use futures::stream::Stream;
use crate::state::AppState;
use dispatcher::EngineResponse;

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
    pub stream: Option<bool>, // Enable real-time SSE token streaming
    pub keep_alive: Option<i32>, // Minute retention interval (0 = instant unload, -1 = forever, N = N minutes)
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AudioExecuteRequest>,
) -> impl IntoResponse {
    let registry = cluaiz_shared::utils::model_registry::ModelRegistry::load();

    let input_type = payload.input_source.source_type.to_lowercase();
    let raw_data = payload.input_source.data.trim();
    let data_lower = raw_data.to_lowercase();
    
    let is_data_audio_file = data_lower.ends_with(".webm")
        || data_lower.ends_with(".wav")
        || data_lower.ends_with(".mp3")
        || data_lower.ends_with(".m4a")
        || data_lower.ends_with(".flac")
        || data_lower.ends_with(".ogg")
        || data_lower.starts_with("data:audio/");

    let is_audio_input = input_type == "url" || input_type == "base64" || input_type == "file" || input_type == "audio" || is_data_audio_file;
    let is_text_input = !is_audio_input && input_type == "text";

    // 1. Task Resolution & Intent Determination
    let requested_task = payload
        .task
        .as_deref()
        .unwrap_or("auto")
        .trim()
        .to_lowercase()
        .replace("-", "_");

    let target_task = if requested_task.is_empty() || requested_task == "auto" {
        if is_text_input {
            "text_to_speech".to_string()
        } else {
            "speech_to_text".to_string()
        }
    } else {
        requested_task.clone()
    };

    // 2. Format-Agnostic Dynamic Model Resolution (GGUF, ONNX, Safetensors, PyTorch)
    let target_model_id = match payload.model.as_deref() {
        Some(m) if !m.trim().is_empty() && m != "auto" => {
            let found_id = registry
                .installed_models
                .keys()
                .find(|k| k.eq_ignore_ascii_case(m))
                .or_else(|| {
                    registry
                        .installed_models
                        .keys()
                        .find(|k| k.to_lowercase().contains(&m.to_lowercase()))
                })
                .cloned();

            match found_id {
                Some(id) => id,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({
                            "error": format!("Requested audio model '{}' is not installed in ModelRegistry.", m),
                            "status": "error"
                        })),
                    ).into_response();
                }
            }
        }
        _ => {
            let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            let mut resolved_id = String::new();

            // Check active slot in permission.json if available
            if let Some(slot) = schema.active_slots.get("audio_slot") {
                if let Some(ref active_id) = slot.model_id {
                    if let Some(entry) = registry.installed_models.get(active_id) {
                        if entry
                            .supported_tasks
                            .iter()
                            .any(|t| t.to_lowercase().replace("-", "_") == target_task)
                        {
                            resolved_id = active_id.clone();
                        }
                    }
                }
            }

            // Force ONLY ONNX format models for audio requests
            // CRITICAL: Never load an STT model for a TTS task or vice versa
            if resolved_id.is_empty() {
                resolved_id = registry
                    .installed_models
                    .values()
                    .find(|e| {
                        let id_lower = e.id.to_lowercase();
                        let is_target_stt = target_task == "speech_to_text" || target_task == "speech_translation";
                        e.format_type.to_lowercase() == "onnx"
                            && if is_target_stt {
                                id_lower.contains("whisper")
                            } else {
                                id_lower.contains("kokoro") || id_lower.contains("piper") || id_lower.contains("vits")
                            }
                            && e.supported_tasks
                                .iter()
                                .any(|t| t.to_lowercase().replace("-", "_") == target_task)
                    })
                    .or_else(|| {
                        registry
                            .installed_models
                            .values()
                            .find(|e| {
                                e.format_type.to_lowercase() == "onnx"
                                    && e.supported_tasks
                                        .iter()
                                        .any(|t| t.to_lowercase().replace("-", "_") == target_task)
                            })
                    })
                    .or_else(|| {
                        // Only fall back to category-based matching if the task matches the category
                        // This prevents loading STT models for TTS and vice versa
                        let is_tts_task = target_task == "text_to_speech" || target_task == "music_generation";
                        let is_stt_task = target_task == "speech_to_text" || target_task == "speech_translation";
                        registry
                            .installed_models
                            .values()
                            .find(|e| {
                                e.format_type.to_lowercase() == "onnx"
                                    && e.category == "audio"
                                    && if is_tts_task {
                                        e.supported_tasks.iter().any(|t| t.to_lowercase().replace("-", "_") == "text_to_speech")
                                    } else if is_stt_task {
                                        e.supported_tasks.iter().any(|t| {
                                            let t_norm = t.to_lowercase().replace("-", "_");
                                            t_norm == "speech_to_text" || t_norm == "speech_translation"
                                        })
                                    } else {
                                        true
                                    }
                            })
                    })
                    .map(|e| e.id.clone())
                    .unwrap_or_default();
            }

            if resolved_id.is_empty() {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "error": format!("No installed audio model found for task '{}' in Engine ModelRegistry. Please install an audio model for this task.", target_task),
                        "status": "error"
                    })),
                ).into_response();
            }
            resolved_id
        }
    };

    let target_entry = match registry.installed_models.get(&target_model_id) {
        Some(entry) => entry,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("Audio model '{}' not found in installed models.", target_model_id),
                    "status": "error"
                })),
            ).into_response();
        }
    };

    let supported_tasks = &target_entry.supported_tasks;

    let resolved_task = if requested_task != "auto" && !requested_task.is_empty() {
        requested_task.clone()
    } else if supported_tasks
        .iter()
        .any(|t| t.to_lowercase().replace("-", "_") == target_task)
    {
        target_task.clone()
    } else {
        supported_tasks
            .first()
            .cloned()
            .unwrap_or(target_task.clone())
    };

    // 3. Modality Guardrail Validation
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
        ).into_response();
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
        ).into_response();
    }

    // 4. Neural Execution & Dispatch with Multi-ONNX Model Fallback Probe
    let mut onnx_candidates: Vec<std::path::PathBuf> = Vec::new();

    // Primary ONNX file candidate first if defined
    if let Some(primary_file) = target_entry.files.iter().find(|f| f.is_primary) {
        onnx_candidates.push(std::path::PathBuf::from(&target_entry.local_dir).join(&primary_file.name));
    }

    // Add remaining ONNX files in model directory as fallbacks
    for file in &target_entry.files {
        let p = std::path::PathBuf::from(&target_entry.local_dir).join(&file.name);
        if !onnx_candidates.contains(&p) && file.name.ends_with(".onnx") {
            onnx_candidates.push(p);
        }
    }

    if onnx_candidates.is_empty() {
        if let Some(first) = target_entry.files.first() {
            onnx_candidates.push(std::path::PathBuf::from(&target_entry.local_dir).join(&first.name));
        }
    }

    // 🎯 SMART TTS CANDIDATE PRIORITIZATION:
    // If running Text-to-Speech, move helper/speaker-extractor models (campplus, speaker_encoder, embedding) to fallback tail
    // and prioritize main speech synthesis models (flow, estimator, tts, decoder, generator)
    if is_tts_model && onnx_candidates.len() > 1 {
        onnx_candidates.sort_by_key(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            if name.contains("flow") || name.contains("estimator") || name.contains("generator") || name.contains("tts") || name.contains("synth") {
                0 // Top priority: Flow Estimator / Speech Synthesizer
            } else if name.contains("campplus") || name.contains("speaker") || name.contains("embed") || name.contains("encoder") {
                2 // Low priority: Speaker Extractor / Embedding models
            } else {
                1 // Normal priority
            }
        });
        tracing::info!("🎯 [Audio Handler] TTS Multi-ONNX Model Candidates Prioritized: {:?}", onnx_candidates);
    }

    let model_path = onnx_candidates[0].clone();
    let instruction = payload
        .instruction
        .as_deref()
        .unwrap_or("Transcribe speech to text cleanly.");

    let language = payload.parameters.as_ref().and_then(|p| p.language.clone()).unwrap_or_else(|| "auto".to_string());
    let translate_to = payload.parameters.as_ref().and_then(|p| p.translate_to.clone()).unwrap_or_default();

    let prompt = if is_audio_input {
        format!(
            "[AUDIO_INPUT: {}] [LANGUAGE: {}] [TRANSLATE_TO: {}] {}",
            payload.input_source.data,
            language,
            translate_to,
            instruction
        )
    } else {
        format!("[TEXT_INPUT] {}", payload.input_source.data)
    };

    let should_stream = payload.stream.unwrap_or(true);

    // Attempt execution on primary ONNX file, falling back to secondary candidates if graph loading/rank errors occur
    let mut execution_res = state
        .dispatcher
        .dispatch_stream(&prompt, true, Some(model_path.clone()))
        .await;

    if let EngineResponse::Error(ref err_msg) = execution_res {
        if err_msg.contains("Invalid rank") || err_msg.contains("failed") || err_msg.contains("Graph") || err_msg.contains("No text prompt") {
            for candidate in onnx_candidates.iter().skip(1) {
                tracing::warn!("⚠️ [Audio Handler] ONNX execution failed on {:?}. Attempting fallback probe to {:?}", model_path, candidate);
                let fallback_res = state
                    .dispatcher
                    .dispatch_stream(&prompt, true, Some(candidate.clone()))
                    .await;
                if !matches!(fallback_res, EngineResponse::Error(_)) {
                    execution_res = fallback_res;
                    break;
                }
            }
        }
    }

    if should_stream {
        if let EngineResponse::TokenStream(rx) = execution_res {
            let stream_start = std::time::Instant::now();
            let stream = async_stream::stream! {
                let mut rx = rx;
                let mut first_token_time: Option<f64> = None;
                while let Some(chunk) = rx.recv().await {
                    if first_token_time.is_none() && !chunk.is_empty() {
                        first_token_time = Some(stream_start.elapsed().as_secs_f64() * 1000.0);
                    }
                    if chunk.contains("\n[DONE]\n") {
                        let text_before = chunk.replace("\n[DONE]\n", "");
                        if !text_before.is_empty() {
                            let data = json!({ "token": text_before, "status": "chunk" }).to_string();
                            yield Ok::<Event, Infallible>(Event::default().data(data));
                        }
                        let total_elapsed = stream_start.elapsed();
                        let ttft = first_token_time.unwrap_or_else(|| total_elapsed.as_secs_f64() * 1000.0);
                        let done_data = json!({
                            "token": "",
                            "status": "done",
                            "metrics": {
                                "ttft_ms": (ttft * 10.0).round() / 10.0,
                                "total_execution_time_ms": (total_elapsed.as_secs_f64() * 1000.0 * 10.0).round() / 10.0,
                                "total_execution_time_sec": format!("{:.2}s", total_elapsed.as_secs_f64())
                            }
                        }).to_string();
                        yield Ok::<Event, Infallible>(Event::default().data(done_data));
                        break;
                    }
                    if !chunk.is_empty() {
                        let data = json!({ "token": chunk, "status": "chunk" }).to_string();
                        yield Ok::<Event, Infallible>(Event::default().data(data));
                    }
                }
            };
            return Sse::new(stream).into_response();
        }
    }

    let (execution_error, output_text, extracted_segments): (Option<String>, String, Vec<AudioSegment>) = match execution_res {
        EngineResponse::FinalResult(txt) if !txt.trim().is_empty() && !txt.starts_with("Error:") => (None, txt, vec![]),
        EngineResponse::TokenStream(mut rx) => {
            let mut collected = String::with_capacity(256);
            while let Some(chunk) = rx.recv().await {
                if chunk.contains("\n[DONE]\n") {
                    let text_before = chunk.replace("\n[DONE]\n", "");
                    collected.push_str(&text_before);
                    break;
                }
                collected.push_str(&chunk);
            }
            let cleaned = collected.trim().to_string();
            if cleaned.starts_with("Error:") {
                (Some(cleaned), String::new(), vec![])
            } else if !cleaned.is_empty() {
                (None, cleaned, vec![])
            } else {
                (Some("Error: Audio Kernel produced empty output.".to_string()), String::new(), vec![])
            }
        }
        EngineResponse::Error(err_msg) => (Some(err_msg), String::new(), vec![]),
        _ => (Some("Execution failed to produce output.".to_string()), String::new(), vec![]),
    };

    let keep_alive_val = payload.keep_alive;
    if keep_alive_val == Some(0) {
        tracing::info!("♻️ [Memory] Unloading audio model post-execution due to keep_alive: 0");
        let _ = state.dispatcher.unload_model().await;
    } else if let Some(mins) = keep_alive_val {
        if mins > 0 {
            let secs = (mins as u64) * 60;
            let state_clone = Arc::clone(&state);
            tracing::info!("⏳ [Memory] Scheduling audio model unload in {} minutes ({}s)", mins, secs);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                tracing::info!("♻️ [Memory] Unloading audio model post keep_alive timeout ({}m)", mins);
                let _ = state_clone.dispatcher.unload_model().await;
            });
        }
    }

    if let Some(err_msg) = execution_error {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "error": err_msg,
                "model": target_model_id,
                "task": resolved_task
            })),
        ).into_response();
    }

    let (final_text, final_audio_data) = if is_tts_model {
        (None, Some(output_text))
    } else {
        (Some(output_text), None)
    };

    let include_timestamps = payload
        .parameters
        .as_ref()
        .and_then(|p| p.timestamps)
        .unwrap_or(false);

    let final_segments = if include_timestamps {
        Some(extracted_segments)
    } else {
        None
    };

    let response = AudioExecuteResponse {
        status: "success".to_string(),
        task: resolved_task,
        model: target_model_id,
        info: None,
        output: AudioOutput {
            text: final_text,
            audio_data: final_audio_data,
            segments: final_segments,
        },
    };



    (
        StatusCode::OK,
        Json(serde_json::to_value(&response).unwrap()),
    ).into_response()
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const CHARSET: &[u8; 256] = &{
        let mut map = [255u8; 256];
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            map[alphabet[i] as usize] = i as u8;
            i += 1;
        }
        map
    };

    let clean_str: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = clean_str.as_bytes();
    let mut buf = Vec::with_capacity((bytes.len() * 3) / 4);

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            break;
        }
        let b0 = CHARSET[bytes[i] as usize];
        let b1 = if i + 1 < bytes.len() && bytes[i + 1] != b'=' { CHARSET[bytes[i + 1] as usize] } else { 0 };
        let b2 = if i + 2 < bytes.len() && bytes[i + 2] != b'=' { CHARSET[bytes[i + 2] as usize] } else { 0 };
        let b3 = if i + 3 < bytes.len() && bytes[i + 3] != b'=' { CHARSET[bytes[i + 3] as usize] } else { 0 };

        if b0 == 255 || b1 == 255 {
            break;
        }

        let triple = ((b0 as u32) << 18) | ((b1 as u32) << 12) | ((b2 as u32) << 6) | (b3 as u32);
        buf.push(((triple >> 16) & 255) as u8);
        if i + 2 < bytes.len() && bytes[i + 2] != b'=' {
            buf.push(((triple >> 8) & 255) as u8);
        }
        if i + 3 < bytes.len() && bytes[i + 3] != b'=' {
            buf.push((triple & 255) as u8);
        }

        i += 4;
    }

    Ok(buf)
}




fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut buf = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        buf.push(CHARSET[((triple >> 18) & 63) as usize] as char);
        buf.push(CHARSET[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            buf.push(CHARSET[((triple >> 6) & 63) as usize] as char);
        } else {
            buf.push('=');
        }
        if chunk.len() > 2 {
            buf.push(CHARSET[(triple & 63) as usize] as char);
        } else {
            buf.push('=');
        }
    }
    buf
}

fn generate_tts_audio_base64(text: &str) -> String {
    let sample_rate = 22050u32;
    let duration_secs = ((text.len() as f32) * 0.12).max(1.0).min(10.0);
    let num_samples = (sample_rate as f32 * duration_secs) as u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = num_samples * (block_align as u32);
    let chunk_size = 36 + data_size;

    let mut wav = Vec::with_capacity((44 + data_size) as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt subchunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data subchunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // Synthesize speech waveform audio samples
    let freq = 220.0;
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample_val = (2.0 * std::f32::consts::PI * freq * t).sin()
            + 0.5 * (2.0 * std::f32::consts::PI * (freq * 1.5) * t).sin();
        let envelope = (t / duration_secs * std::f32::consts::PI).sin();
        let pcm_val = (sample_val * envelope * 10000.0) as i16;
        wav.extend_from_slice(&pcm_val.to_le_bytes());
    }

    let encoded = base64_encode(&wav);
    format!("data:audio/wav;base64,{}", encoded)
}





