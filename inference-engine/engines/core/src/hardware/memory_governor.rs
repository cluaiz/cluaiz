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

/// Dynamically calculates the safe maximum system memory utilization ceiling (0.88 - 0.96)
/// based on real-time background memory load ratio.
pub fn calculate_dynamic_system_ceiling_pct(total_ram_gb: f64, available_ram_gb: f64) -> f64 {
    let used_ram_gb = (total_ram_gb - available_ram_gb).max(0.0);
    let load_ratio = if total_ram_gb > 0.0 {
        (used_ram_gb / total_ram_gb).clamp(0.0, 1.0)
    } else {
        0.5
    };

    // When background apps are light (< 35%), safe ceiling expands up to 96%.
    // When moderate (35% - 65%), ceiling smoothly adapts between 92% and 95%.
    // When heavy (> 65%), ceiling tightens to 88% - 92% to protect existing processes.
    if load_ratio < 0.35 {
        0.96
    } else if load_ratio <= 0.65 {
        0.95 - ((load_ratio - 0.35) / 0.30) * 0.03
    } else {
        0.92 - ((load_ratio - 0.65) / 0.35) * 0.04
    }
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
        let ceiling_pct = calculate_dynamic_system_ceiling_pct(total_ram_gb, available_ram_gb);
        let max_allowed_system_ram = total_ram_gb * ceiling_pct;
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

    (total_vram_gb * 0.08).clamp(min_vram_guard, 1.00)
}

/// Calculates the OS safety buffer for CPU RAM in GB based on real-time system metrics.
/// Zero hardcoding: dynamically evaluates current system load ratio and reserves 5% - 15%.
pub fn calculate_ram_safety_buffer(
    opt_control: &OptimizationControl,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> f64 {
    if let Some(direct_gb) = opt_control.custom_ram_buffer_gb {
        if direct_gb > 0.0 {
            let max_allowed = (total_ram_gb - 2.0).max(1.0);
            return direct_gb.max(1.00).min(max_allowed);
        }
    }

    // Dynamic Real-Time Safety Buffer Calculation (Zero Hardcoding)
    let used_ram_gb = (total_ram_gb - available_ram_gb).max(0.0);
    let load_ratio = if total_ram_gb > 0.0 {
        (used_ram_gb / total_ram_gb).clamp(0.0, 1.0)
    } else {
        0.5
    };

    // Derive dynamic safety percentage:
    // Light load (< 35% used): 5%
    // Moderate load (35% - 65% used): scales from 5% to 10%
    // Heavy load (> 65% used): scales from 10% to 15%
    let dynamic_pct = if load_ratio < 0.35 {
        0.05
    } else if load_ratio <= 0.65 {
        0.05 + ((load_ratio - 0.35) / 0.30) * 0.05
    } else {
        0.10 + ((load_ratio - 0.65) / 0.35) * 0.05
    };

    (total_ram_gb * dynamic_pct).max(1.00)
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
        let ceiling_pct = calculate_dynamic_system_ceiling_pct(total_ram_gb, available_ram_gb);
        let max_allowed_system_ram = total_ram_gb * ceiling_pct;
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
