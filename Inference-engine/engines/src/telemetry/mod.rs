pub mod health_check;
pub mod sentinel;

/// 📡 Telemetry Ignition: Starts the background telemetry server (Port 3030).
pub fn ignite_watchtower() {
    let pulse = cluaiz_shared::hardware::telemetry::get_pulse();
    tokio::spawn(async move {
        let server = crate::api::server::TelemetryServer::new(pulse);
        if let Err(e) = server.start(3030).await {
            tracing::error!("🛡️ [Watchtower] Telemetry Server failed: {}", e);
        }
    });
}

pub struct TelemetryService;
