//! 🚀 Sovereign Speed Checker: Hardware-Model Synergy Auditor
//! Predicts performance tier based on neural architecture and silicon truth.

use serde::{Deserialize, Serialize};
use crate::hardware::schema::profiles::SovereignProfile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum HealthStatus {
    Panic,
    Critical,
    Lagging,
    Moderate,
    Instant,
    HyperSpeed,
    GodMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub status: HealthStatus,
    pub expected_tps: f64,
    pub bottleneck: String,
}

/// 🧬 SOVEREIGN AUDIT: Analyzes the interaction between model requirements and local silicon.
pub fn predict_performance(
    parameters: &str,
    bit_depth: f64,
    _context_window: &str,
    requires_gpu: bool,
    hardware: &SovereignProfile,
) -> PerformanceReport {
    let mut score = 100.0;
    let mut bottleneck = "NONE".to_string();

    // ── 1. Parameter Pressure ──
    let param_count = parameters.trim_end_matches('B').parse::<f64>().unwrap_or(7.0);
    if param_count > 30.0 {
        score -= 40.0;
        bottleneck = "Compute Density".to_string();
    } else if param_count > 10.0 {
        score -= 20.0;
    }

    // ── 2. Quantization Advantage ──
    if bit_depth < 2.0 {
        score += 30.0; // BitNet/Binary optimization
    } else if bit_depth > 4.0 {
        score -= 15.0; // High precision overhead
    }

    // ── 3. Silicon Dispatch Audit ──
    if requires_gpu {
        if !hardware.compute.has_gpu {
            return PerformanceReport {
                status: HealthStatus::Panic,
                expected_tps: 0.5,
                bottleneck: "Missing Accelerator".to_string(),
            };
        }
        
        if hardware.compute.vram_gb < (param_count * bit_depth / 8.0) {
            score -= 50.0;
            bottleneck = "VRAM Pressure".to_string();
        } else {
            score += 20.0;
        }
    } else {
        // CPU Inference
        if hardware.cpu_cores < 8 {
            score -= 30.0;
            bottleneck = "Core Starvation".to_string();
        }
    }

    // ── 4. Final Tier Resolution ──
    let (status, tps) = match score {
        s if s >= 130.0 => (HealthStatus::GodMode, 85.0),
        s if s >= 110.0 => (HealthStatus::HyperSpeed, 55.0),
        s if s >= 90.0  => (HealthStatus::Instant, 35.0),
        s if s >= 70.0  => (HealthStatus::Moderate, 15.0),
        s if s >= 40.0  => (HealthStatus::Lagging, 5.0),
        _               => (HealthStatus::Critical, 1.5),
    };

    PerformanceReport {
        status,
        expected_tps: tps,
        bottleneck,
    }
}
