//! Learned VRAM numbers. llama.cpp keeps a fixed 1024 MiB free on a device (`--fit-target`)
//! because part of what a model takes is not in any estimate. Two measurements replace it:
//! - the **correction**: llama.cpp's own dry run of the model on the GPU (model + context +
//!   compute buffers) minus the planner's estimate of the same placement, taken once per model;
//! - the **pool gap**: what the loaded model really took after a full-batch warmup (the GPU pool
//!   grows there) minus the corrected estimate, kept at its largest.
//! A model measured on this machine is planned with both plus a small desktop reserve; a model never
//! measured still gets llama.cpp's rule. The user's `custom_vram_buffer_gb` replaces the margin.

use cluaiz_core::{ActiveUserSettings, FeatureState};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use std::collections::HashMap;
use std::sync::RwLock;

/// Share of the GPU's total memory left for the desktop and other apps once a model's real size
/// is known (0.20 GB on a 4 GB GPU), kept within [`DEVICE_RESERVE_MIN_GB`, `DEVICE_RESERVE_MAX_GB`]:
/// what other apps add after a load does not grow with the GPU (5% of 128 GB would be 6.4 GB)
pub const DEVICE_RESERVE_SHARE: f64 = 0.05;
pub const DEVICE_RESERVE_MIN_GB: f64 = 0.15;
pub const DEVICE_RESERVE_MAX_GB: f64 = 1.0;

/// Key prefix of a dry-run correction. It is relative to the estimate, so a change to the estimate
/// (v2: hybrid models count KV only for attention layers) needs new ones: older ones are dropped
const CORRECTION: &str = "proj2:";

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

/// The measured pool gap for this model and settings, if it was ever loaded on the GPU here
pub fn learned(geometry: &ModelGeometry, settings: &ActiveUserSettings) -> Option<f64> {
    learned_key(&format!("pool:{}", key(geometry, settings)))
}

/// llama.cpp's dry run minus the planner's estimate for this model's GPU bytes, if it was run here
pub fn correction(geometry: &ModelGeometry, settings: &ActiveUserSettings) -> Option<f64> {
    learned_key(&format!("{CORRECTION}{}", key(geometry, settings)))
}

/// Keeps a correction (replaces an earlier one: the dry run is exact, not a peak)
pub fn record_correction(geometry: &ModelGeometry, settings: &ActiveUserSettings, gb: f64) {
    if !gb.is_finite() {
        return;
    }
    if let Ok(mut guard) = LEARNED.write() {
        guard.get_or_insert_with(HashMap::new).insert(format!("{CORRECTION}{}", key(geometry, settings)), gb);
    }
}

/// A measurement by [`key`] or by another owner's key (an ONNX package's device use)
pub fn learned_key(key: &str) -> Option<f64> {
    LEARNED.read().ok()?.as_ref()?.get(key).copied()
}

/// Records a measured pool gap; the largest one seen is kept (a later load may hit a bigger peak)
pub fn record(geometry: &ModelGeometry, settings: &ActiveUserSettings, gap_gb: f64) -> bool {
    record_key(&pool_key(geometry, settings), gap_gb)
}

/// The key a pool gap is kept under
pub fn pool_key(geometry: &ModelGeometry, settings: &ActiveUserSettings) -> String {
    format!("pool:{}", key(geometry, settings))
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

/// Measurements read back from disk (replaces what is held); entries of an older format are dropped
pub fn restore(mut measured: HashMap<String, f64>) {
    measured.retain(|key, _| ["pool:", CORRECTION, "onnx:", "speech:"].iter().any(|prefix| key.starts_with(prefix)));
    if let Ok(mut guard) = LEARNED.write() {
        *guard = Some(measured);
    }
}

/// GB kept free for the desktop on this GPU once the model's real size is known
pub fn reserve_gb(hardware: &SystemInfo) -> f64 {
    hardware.gpus.first().map_or(0.0, |g| {
        (g.vram_total_gb * DEVICE_RESERVE_SHARE).clamp(DEVICE_RESERVE_MIN_GB, DEVICE_RESERVE_MAX_GB)
    })
}

/// Where the VRAM buffer of a plan comes from, for the plan report
pub fn describe(geometry: &ModelGeometry, settings: &ActiveUserSettings, hardware: &SystemInfo, buffer_gb: f64) -> String {
    if settings.optimization.custom_vram_buffer_gb.is_some() {
        return "user setting".to_string();
    }
    let corrected = correction(geometry, settings)
        .map(|c| format!("; estimate corrected by llama.cpp's dry run {:+.2} GB", c))
        .unwrap_or_default();
    match learned(geometry, settings) {
        Some(gap) => format!("measured: GPU pool {:+.2} GB + {:.2} GB reserve{}", gap, reserve_gb(hardware), corrected),
        None => format!("{:.0} MiB rule, model not measured on this machine yet{}", buffer_gb * 1024.0, corrected),
    }
}
