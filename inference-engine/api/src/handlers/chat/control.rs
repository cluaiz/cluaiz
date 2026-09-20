use axum::{response::IntoResponse, Json};
use serde_json::json;
use std::sync::atomic::Ordering;
use super::types::{StreamControlRequest, ACTIVE_STREAMS};

// ─── POST /v1/chat/cancel (Cancel Active Stream) ──────────────────────
pub async fn cancel_chat_stream(
    Json(payload): Json<StreamControlRequest>,
) -> axum::response::Response {
    let signal_found = if let Ok(lock) = ACTIVE_STREAMS.read() {
        if let Some(entry) = lock.get(&payload.stream_id) {
            entry.cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    } else {
        false
    };

    if signal_found {
        // 🛑 Dual-Layer Signal: Trigger Deep Hardware Native C++ Llama Engine Interrupt
        cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
        tracing::info!("🛑 [StreamControl] Deep cancel signal dispatched for stream '{}'.", payload.stream_id);
        Json(json!({
            "status": "cancelled",
            "stream_id": payload.stream_id,
            "message": "Stream cancellation signal dispatched successfully."
        })).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "message": format!("Active stream '{}' not found or already completed.", payload.stream_id),
                    "type": "invalid_request_error",
                    "code": "stream_not_found"
                }
            }))
        ).into_response()
    }
}

// ─── POST /v1/chat/skip-reasoning (Fast-Forward Thinking) ─────────────
pub async fn skip_chat_reasoning(
    Json(payload): Json<StreamControlRequest>,
) -> axum::response::Response {
    let signal_found = if let Ok(lock) = ACTIVE_STREAMS.read() {
        if let Some(entry) = lock.get(&payload.stream_id) {
            entry.skip_reasoning.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    } else {
        false
    };

    if signal_found {
        // ⏩ Dual-Layer Signal: Trigger Deep Hardware Native C++ Llama Reasoning Exit
        cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.store(true, Ordering::SeqCst);
        tracing::info!("⏩ [StreamControl] Deep skip-reasoning signal dispatched for stream '{}'.", payload.stream_id);
        Json(json!({
            "status": "skipped",
            "stream_id": payload.stream_id,
            "message": "Skip reasoning signal dispatched successfully."
        })).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "message": format!("Active stream '{}' not found or already completed.", payload.stream_id),
                    "type": "invalid_request_error",
                    "code": "stream_not_found"
                }
            }))
        ).into_response()
    }
}
