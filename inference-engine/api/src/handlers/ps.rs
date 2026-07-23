use axum::{
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response, Sse, sse::Event},
    Json,
};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};
use crate::AppState;
use cluaiz_shared::hardware::governor::HardwareGovernor;

fn gather_ps_data() -> Value {
    let active_processes = HardwareGovernor::get_active_allocations();
    let roster = engines::CoreRoster::load_roster();
    let mut processes = Vec::new();
    for info in active_processes {
        let original_ctx = roster.iter()
            .find(|m| m.id == info.model_id || m.huggingface_filename == info.model_id || m.id.contains(&info.model_id))
            .map(|m| m.context_window.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        processes.push(json!({
            "pid": info.pid.to_string(),
            "model_id": info.model_id,
            "vram_gb": info.vram_gb,
            "context_size": info.context_size,
            "original_context": original_ctx,
            "engine": info.engine
        }));
    }

    let mut pulse_json = json!({});
    if let Ok(lock) = cluaiz_shared::hardware::telemetry::get_pulse().pulse.read() {
        pulse_json = serde_json::to_value(&*lock).unwrap_or(json!({}));
    }

    // Include full hardware snapshot alongside processes
    json!({
        "status": "success",
        "active_processes": processes,
        "hardware_snapshot": pulse_json
    })
}

// ─── GET /v1/system/ps ────────────────────────────────────────────────
pub async fn get_processes(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>
) -> Response {
    if let Some(accept) = headers.get(header::ACCEPT) {
        if accept == "text/event-stream" {
            let stream = async_stream::stream! {
                loop {
                    let data = gather_ps_data();
                    yield Ok::<_, Infallible>(Event::default().json_data(data).unwrap());
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            };
            return Sse::new(stream).into_response();
        }
    }
    
    // FFI & standard fallback: Static JSON response
    Json(gather_ps_data()).into_response()
}
