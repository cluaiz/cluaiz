//! Learned VRAM margin. llama.cpp keeps a fixed 1024 MiB free on a device (`--fit-target`) because
//! part of what a model takes is not in any estimate: llama.cpp's compute buffers, and the CUDA
//! pool that grows during the first full batch (about 0.6 GB for Qwen3-4B at ubatch 512 on an
//! RTX 3050). The runtime measures what a loaded model really took after a full-batch warmup and
//! records it here as the gap to the planner's estimate. A model measured on this machine is then
//! planned with that gap plus a small reserve for the desktop, not with the fixed 1024 MiB; a model
//! never measured still gets llama.cpp's rule. The user's `custom_vram_buffer_gb` replaces both.

use cluaiz_core::{ActiveUserSettings, FeatureState};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use std::collections::HashMap;
use std::sync::RwLock;

/// Share of the GPU's total memory left for the desktop and other apps once a model's real size
/// is known (0.20 GB on a 4 GB GPU)
pub const DEVICE_RESERVE_SHARE: f64 = 0.05;

/// Measured gap per model + load settings, in GB (negative: the estimate was too high)
static LEARNED: RwLock<Option<HashMap<String, f64>>> = RwLock::new(None);

/// What identifies a measurement: the model file (architecture, size, blocks) and the settings
/// that change llama.cpp's buffers (micro-batch, flash attention)
pub fn key(geometry: &ModelGeometry, settings: &ActiveUserSettings) -> String {
    let fa = match settings.optimization.flash_attention {
        FeatureState::On => "on",
        FeatureState::Off => "off",
        _ => "auto",
    };
    format!(
        "{}:{}:{}:ub{}:fa-{}",
        geometry.architecture,
        geometry.file_size_bytes,
        geometry.block_count,
        settings.optimization.resolved_ubatch_size(),
        fa
    )
}

/// The measured gap for this model and settings, if it was ever loaded on the GPU here
pub fn learned(geometry: &ModelGeometry, settings: &ActiveUserSettings) -> Option<f64> {
    LEARNED.read().ok()?.as_ref()?.get(&key(geometry, settings)).copied()
}

/// Records a measured gap; the largest one seen is kept (a later load may hit a bigger peak)
pub fn record(geometry: &ModelGeometry, settings: &ActiveUserSettings, gap_gb: f64) -> bool {
    record_key(&key(geometry, settings), gap_gb)
}

/// [`record`] by [`key`]; true when the kept value changed
pub fn record_key(key: &str, gap_gb: f64) -> bool {
    if !gap_gb.is_finite() {
        return false;
    }
    let Ok(mut guard) = LEARNED.write() else { return false };
    let map = guard.get_or_insert_with(HashMap::new);
    match map.get_mut(key) {
        Some(kept) if *kept >= gap_gb => false,
        Some(kept) => {
            *kept = gap_gb;
            true
        }
        None => {
            map.insert(key.to_string(), gap_gb);
            true
        }
    }
}

/// Every measurement, to save
pub fn snapshot() -> HashMap<String, f64> {
    LEARNED.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Measurements read back from disk (replaces what is held)
pub fn restore(measured: HashMap<String, f64>) {
    if let Ok(mut guard) = LEARNED.write() {
        *guard = Some(measured);
    }
}

/// GB kept free for the desktop on this GPU once the model's real size is known
pub fn reserve_gb(hardware: &SystemInfo) -> f64 {
    hardware.gpus.first().map_or(0.0, |g| g.vram_total_gb * DEVICE_RESERVE_SHARE)
}

/// Where the VRAM buffer of a plan comes from, for the plan report
pub fn describe(geometry: &ModelGeometry, settings: &ActiveUserSettings, hardware: &SystemInfo, buffer_gb: f64) -> String {
    if settings.optimization.custom_vram_buffer_gb.is_some() {
        return "user setting".to_string();
    }
    match learned(geometry, settings) {
        Some(gap) => format!("measured: {:+.2} GB over the estimate + {:.2} GB reserve", gap, reserve_gb(hardware)),
        None => format!("{:.0} MiB rule, model not measured on this machine yet", buffer_gb * 1024.0),
    }
}
