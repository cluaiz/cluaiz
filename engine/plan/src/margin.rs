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

/// What the GPU pool may still grow after the load of a model not measured yet, once llama.cpp's dry
/// run has corrected the estimate (model + context + compute buffers). Measured pool gaps on the
/// 4 GB laptop: 0.02-0.23 GB (run 20)
pub const UNMEASURED_POOL_GB: f64 = 0.30;

/// Key prefix of a dry-run correction. It is relative to the estimate, so a change to the estimate
/// needs new ones: older ones are dropped. v2: hybrid models count KV only for attention layers;
/// v3: sliding-window layers follow llama.cpp's default pattern when the file lists none
const CORRECTION: &str = "proj3:";
/// Key prefix of a pool gap: what a load took minus the corrected estimate, so it follows the
/// estimate too (v2 with the correction's v3; a stale gap of -4 GB under-planned Gemma 3 12B by
/// 0.64 GB in run 21)
const POOL: &str = "pool2:";

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
    learned_key(&format!("{POOL}{}", key(geometry, settings)))
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
    format!("{POOL}{}", key(geometry, settings))
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

/// Keeps a measurement by key, replacing an earlier one (a speed is the latest run, not a peak)
pub fn set_key(key: &str, value: f64) {
    if let Ok(mut guard) = LEARNED.write() {
        guard.get_or_insert_with(HashMap::new).insert(key.to_string(), value);
    }
}

/// Drops every measurement of a removed model: its GGUF files' VRAM and speed numbers (by
/// `geometries`) and its ONNX / speech package numbers (by `model_id`). Returns how many were dropped.
pub fn forget_model(model_id: &str, geometries: &[ModelGeometry]) -> usize {
    let files: Vec<String> = geometries
        .iter()
        .map(|g| format!("{}:{}:{}:", g.architecture, g.file_size_bytes, g.block_count))
        .collect();
    let package = format!("onnx:{model_id}:");
    let speech = format!("speech:{model_id}");
    let of_model = |key: &str| {
        let rest = [POOL, CORRECTION, "speed:tg:", "speed:pp:"]
            .iter()
            .find_map(|prefix| key.strip_prefix(prefix));
        rest.is_some_and(|rest| files.iter().any(|file| rest.starts_with(file.as_str())))
            || key.starts_with(&package)
            || key == speech
    };
    let Ok(mut guard) = LEARNED.write() else { return 0 };
    let Some(map) = guard.as_mut() else { return 0 };
    let before = map.len();
    map.retain(|key, _| !of_model(key));
    before - map.len()
}

/// Every measurement, to save
pub fn snapshot() -> HashMap<String, f64> {
    LEARNED.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Measurements read back from disk (replaces what is held); entries of an older format are dropped
pub fn restore(mut measured: HashMap<String, f64>) {
    measured.retain(|key, _| [POOL, CORRECTION, "onnx:", "speech:", crate::speed::PREFIX].iter().any(|prefix| key.starts_with(prefix)));
    if let Ok(mut guard) = LEARNED.write() {
        *guard = Some(measured);
    }
}

/// True when a load's VRAM measurement cannot be trusted: the plan needed more than was free, or
/// the GPU was left nearly full. On Windows the driver then spills into shared system memory, the
/// free VRAM drops by less than the model took, and the learned gap comes out falsely negative.
pub fn looks_spilled(planned_gb: f64, free_before_gb: f64, free_after_gb: f64) -> bool {
    planned_gb > free_before_gb || free_after_gb < DEVICE_RESERVE_MIN_GB
}

/// A learned gap no load can produce: the estimate cannot be too high by more than the whole GPU
pub fn plausible_gap(gap_gb: f64, hardware: &SystemInfo) -> bool {
    hardware.gpus.first().is_none_or(|g| gap_gb >= -g.vram_total_gb)
}

/// GB kept free for the desktop on this GPU once the model's real size is known. A GPU measured as
/// used by nothing else (no display, no other process: a server GPU) keeps only the minimum, as no
/// other app takes its memory after the load; when that is not known the desktop share stays.
pub fn reserve_gb(hardware: &SystemInfo) -> f64 {
    hardware.gpus.first().map_or(0.0, |g| {
        if g.used_only_by_cluaiz == Some(true) {
            return DEVICE_RESERVE_MIN_GB;
        }
        (g.vram_total_gb * DEVICE_RESERVE_SHARE).clamp(DEVICE_RESERVE_MIN_GB, DEVICE_RESERVE_MAX_GB)
    })
}

/// Gap for a model not measured on this GPU yet whose estimate llama.cpp's dry run corrected: the
/// desktop reserve plus [`UNMEASURED_POOL_GB`], at most llama.cpp's own 1024 MiB rule. 0.50 GB on a
/// 4 GB GPU, 1.0 GB from 12 GB up
pub fn card_gap_gb(hardware: &SystemInfo) -> f64 {
    (reserve_gb(hardware) + UNMEASURED_POOL_GB).min(DEVICE_RESERVE_MAX_GB)
}

/// Where the VRAM buffer of a plan comes from, for the plan report
pub fn describe(geometry: &ModelGeometry, settings: &ActiveUserSettings, hardware: &SystemInfo, buffer_gb: f64) -> String {
    if settings.optimization.custom_vram_buffer_gb.is_some() {
        return "user setting".to_string();
    }
    let usual = crate::planner::model_buffer_options(geometry, hardware, settings).into_iter().fold(f64::MAX, f64::min);
    if buffer_gb + 1e-9 < usual {
        return format!(
            "tight gap: {:.2} GB instead of {:.2} GB, because the usual gap would put less of the model on the GPU; a load that fails steps back one placement",
            buffer_gb, usual
        );
    }
    let corrected = correction(geometry, settings)
        .map(|c| format!("; estimate corrected by llama.cpp's dry run {:+.2} GB", c))
        .unwrap_or_default();
    match learned(geometry, settings) {
        Some(gap) if !plausible_gap(gap, hardware) => format!(
            "{:.2} GB as for a model not measured yet; a measured GPU pool of {:+.2} GB is ignored (larger than the GPU, from a load that spilled into shared memory){}",
            buffer_gb, gap, corrected
        ),
        Some(gap) => {
            let alone = hardware.gpus.first().is_some_and(|g| g.used_only_by_cluaiz == Some(true));
            let whose = if alone { " (GPU used by nothing else)" } else { "" };
            format!("measured: GPU pool {:+.2} GB + {:.2} GB reserve{}{}", gap, reserve_gb(hardware), whose, corrected)
        }
        None if correction(geometry, settings).is_some() && !hardware.gpus.is_empty() => format!(
            "card-sized gap: {:.2} GB reserve + {:.2} GB for the GPU pool, model not measured on this machine yet{}",
            reserve_gb(hardware), UNMEASURED_POOL_GB, corrected
        ),
        None => format!("{:.0} MiB rule, model not measured on this machine yet{}", buffer_gb * 1024.0, corrected),
    }
}
