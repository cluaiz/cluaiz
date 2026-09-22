//! Archer Universal Orchestrator: Common engine lifecycle controller.

use crate::hardware::telemetry::{EngineGear, ObservableHardwareState};
use std::sync::Arc;

/// The central contract for all Archer-class engines.
/// Ensures that hardware pressure (Gears) and memory management (Relay) 
/// are handled identically across all backends.
pub trait SovereignOrchestrator {
    /// Returns the current hardware pulse associated with the engine.
    fn pulse(&self) -> &Arc<ObservableHardwareState>;

    /// Executes the standard heartbeat check.
    /// Returns the active EngineGear resolved from silicon sensors.
    fn heartbeat(&self) -> EngineGear {
        self.pulse().resolve_gear()
    }

    /// Logs a gear shift event in a standardized format.
    fn log_shift(&self, gear: EngineGear, context: &str) -> Result<(), &'static str> {
        match gear {
            EngineGear::Pulse => Ok(()),
            EngineGear::Balanced => {
                tracing::info!("⚙️ [ORCHESTRATOR] Gear 2 (Balanced) sync: {}", context);
                Ok(())
            },
            EngineGear::Survival => {
                tracing::warn!("🛡️ [ORCHESTRATOR] Gear 3 (Survival) active: {}", context);
                Ok(())
            },
            EngineGear::Emergency => {
                tracing::error!("🚨 [ORCHESTRATOR] Gear 4 (Emergency) limit hit: {}", context);
                Ok(())
            },
        }
    }
}
