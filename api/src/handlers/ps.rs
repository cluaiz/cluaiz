//! 📊 Process & Active Model Telemetry Handler (`api/src/handlers/ps.rs`)
//! Modern single-source process tracking using cluaiz-hw and active execution plans.

use axum::{
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response, Sse, sse::Event},
    Json,
};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};
use crate::AppState;

fn gather_ps_data(plan_opt: Option<cluaiz_plan::ExecutionPlan>) -> Value {
    let mut processes = Vec::new();
    if let Some(plan) = plan_opt {
        processes.push(json!({
            "pid": std::process::id().to_string(),
            "model_id": plan.model_id,
            "vram_gb": plan.vram_budget_gb,
            "context_size": plan.context_window,
            "tier": format!("{:?}", plan.selected_tier),
            "n_gpu_layers": plan.n_gpu_layers,
            "engine": "llama-engine (static rlib)"
        }));
    }

    let hw = cluaiz_hw::HardwareProber::current();
    json!({
        "status": "success",
        "active_processes": processes,
        "hardware_snapshot": hw
    })
}

/// GET /v1/system/ps
pub async fn get_processes(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    let plan = state.host.current_plan.lock().await.clone();

    if let Some(accept) = headers.get(header::ACCEPT) {
        if accept == "text/event-stream" {
            let stream = async_stream::stream! {
                loop {
                    let data = gather_ps_data(plan.clone());
                    yield Ok::<_, Infallible>(Event::default().json_data(data).unwrap());
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            };
            return Sse::new(stream).into_response();
        }
    }

    Json(gather_ps_data(plan)).into_response()
}
