//! What models really took on this machine's GPU (`cluaiz_plan::margin`), kept in
//! `engine/vram_margins.json` so a restart plans with it on the first load.

/// Where the measurements are kept
fn path() -> std::path::PathBuf {
    cluaiz_core::EnvironmentManager::current().config_dir().join("vram_margins.json")
}

/// Reads the measurements once per process
pub fn load() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let measured = std::fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str::<std::collections::HashMap<String, f64>>(&text).ok())
            .unwrap_or_default();
        cluaiz_plan::margin::restore(measured);
    });
}

pub fn save() {
    let path = path();
    let saved = serde_json::to_string_pretty(&cluaiz_plan::margin::snapshot())
        .map_err(anyhow::Error::from)
        .and_then(|text| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Ok(std::fs::write(&path, text)?)
        });
    if let Err(e) = saved {
        tracing::warn!("⚠️ [VRAM] Could not save the measured VRAM use to {}: {}", path.display(), e);
    }
}

/// Before an installed model is deleted: drops its measurements (VRAM, speed) from
/// `vram_margins.json`, so a removed model leaves nothing behind there. Its GGUF files are read
/// for their geometry, so this runs while they still exist.
pub fn forget_model(model_id: &str) {
    load();
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let Some(entry) = registry.installed_models.get(model_id) else { return };
    let mut geometries = Vec::new();
    let mut dirs = vec![std::path::PathBuf::from(&entry.local_dir)];
    // The model folder and one level below it (split or projector files)
    for depth in 0..2 {
        let mut next = Vec::new();
        for dir in &dirs {
            for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = file.path();
                if path.is_dir() && depth == 0 {
                    next.push(path);
                } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gguf")) {
                    if let Ok(geometry) = cluaiz_models::GgufProber::probe_geometry(&path) {
                        geometries.push(geometry);
                    }
                }
            }
        }
        dirs = next;
    }
    let dropped = cluaiz_plan::margin::forget_model(model_id, &geometries);
    if dropped > 0 {
        save();
        tracing::info!("🧹 [VRAM] Dropped {} measurement(s) of the removed model '{}'", dropped, model_id);
    }
}

/// Free VRAM of the first GPU now, in GB (0 without a GPU)
pub fn free_gb() -> f64 {
    cluaiz_hw::HardwareProber::with_live_memory().gpus.first().map_or(0.0, |g| g.vram_available_gb)
}
