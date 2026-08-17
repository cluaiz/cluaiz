//! 🛡️ Memory Governor Safety Buffers
//! Centrally calculates system margins and usable bounds to prevent OOMs during inference.
//!
//! ### System Overview
//! The Memory Governor serves as the Single Source of Truth for memory allocation safety:
//! - **VRAM Buffers:** Enforces user settings with a strict 250MB safe floor constraint.
//! - **RAM Buffers:** Enforces user settings with a minimum 1.00 GB safe floor to prevent OS crashes.
//! - **Auto Mode:** Automatically clamps margins dynamically based on system limits.
//! - **Usable Bounds:** Evaluates actual safe allocation targets (`MemoryDecision`) for all models globally.


use crate::hardware::schema::optimization::OptimizationControl;

/// Consolidated memory decision containing safety buffers and usable memory targets.
#[derive(Debug, Clone)]
pub struct MemoryDecision {
    pub usable_vram_gb: f64,
    pub usable_ram_gb: f64,
    pub vram_safety_gb: f64,
    pub ram_safety_gb: f64,
}

/// Computes the final usable VRAM in GB after applying safety buffers.
pub fn calculate_usable_vram(
    opt_control: &OptimizationControl,
    total_vram_gb: f64,
    live_free_vram_gb: f64,
) -> f64 {
    let safety = calculate_safety_buffer(opt_control, total_vram_gb, live_free_vram_gb);
    (live_free_vram_gb - safety).max(0.0)
}

/// Computes the final usable RAM in GB after applying safety buffers and system ceilings.
pub fn calculate_usable_ram(
    opt_control: &OptimizationControl,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> f64 {
    let ram_safety_gb = calculate_ram_safety_buffer(opt_control, total_ram_gb, available_ram_gb);

    if opt_control.custom_ram_buffer_gb.is_some() {
        (available_ram_gb - ram_safety_gb).max(0.0)
    } else {
        let max_allowed_system_ram = (total_ram_gb - ram_safety_gb).max(0.0);
        let pre_existing_used_ram = (total_ram_gb - available_ram_gb).max(0.0);
        let system_cap_usable_ram = (max_allowed_system_ram - pre_existing_used_ram).max(0.0);
        let raw_usable_ram = (available_ram_gb - ram_safety_gb).max(0.0);
        raw_usable_ram.min(system_cap_usable_ram).max(0.0)
    }
}

/// Calculates the OS safety buffer in VRAM in GB based on user settings.
pub fn calculate_safety_buffer(
    opt_control: &OptimizationControl,
    total_vram_gb: f64,
    _live_free_vram_gb: f64,
) -> f64 {
    let min_vram_guard = 0.25f64; // Minimum 250MB safe floor

    if let Some(direct_gb) = opt_control.custom_vram_buffer_gb {
        if direct_gb > 0.0 {
            let max_allowed = (total_vram_gb - 0.25).max(0.0);
            return direct_gb.max(min_vram_guard).min(max_allowed);
        }
    }

    (total_vram_gb * 0.05).clamp(min_vram_guard, 1.00)
}

/// Calculates the OS safety buffer for CPU RAM in GB based on user settings.
pub fn calculate_ram_safety_buffer(
    opt_control: &OptimizationControl,
    total_ram_gb: f64,
    _available_ram_gb: f64,
) -> f64 {
    let min_ram_guard = 1.00f64;

    if let Some(direct_gb) = opt_control.custom_ram_buffer_gb {
        if direct_gb > 0.0 {
            return direct_gb.max(min_ram_guard);
        }
    }

    let auto_buffer = (total_ram_gb * 0.05).clamp(1.00, 1.50);
    auto_buffer
}

/// Computes the final unified `MemoryDecision` based on system hardware stats and user configurations.
pub fn get_memory_decision(
    opt_control: &OptimizationControl,
    total_vram_gb: f64,
    live_free_vram_gb: f64,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> MemoryDecision {
    let vram_safety_gb = calculate_safety_buffer(opt_control, total_vram_gb, live_free_vram_gb);
    let ram_safety_gb = calculate_ram_safety_buffer(opt_control, total_ram_gb, available_ram_gb);

    let usable_vram_gb = (live_free_vram_gb - vram_safety_gb).max(0.0);

    let usable_ram_gb = if opt_control.custom_ram_buffer_gb.is_some() {
        (available_ram_gb - ram_safety_gb).max(0.0)
    } else {
        let max_allowed_system_ram = (total_ram_gb - ram_safety_gb).max(0.0);
        let pre_existing_used_ram = (total_ram_gb - available_ram_gb).max(0.0);
        let system_cap_usable_ram = (max_allowed_system_ram - pre_existing_used_ram).max(0.0);
        let raw_usable_ram = (available_ram_gb - ram_safety_gb).max(0.0);
        raw_usable_ram.min(system_cap_usable_ram).max(0.0)
    };

    MemoryDecision {
        usable_vram_gb,
        usable_ram_gb,
        vram_safety_gb,
        ram_safety_gb,
    }
}
