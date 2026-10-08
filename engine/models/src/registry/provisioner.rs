//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Model Provisioner
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;
use tracing::info;
use crate::models::types::manifest::ModelManifest;

pub struct Provisioner;

impl Provisioner {
    /// Discovers and initialises local models found in the models directory
    pub fn provision_models(base_path: &Path) -> Vec<ModelManifest> {
        info!("🚀 [Provisioner] Initialising model registry from {}", base_path.display());
        let manifests = crate::models::registry::discovery::AutonomousDiscovery::index_Cluaiz_models(base_path);
        info!("✅ [Provisioner] Provisioned {} model manifests", manifests.len());
        manifests
    }
}
