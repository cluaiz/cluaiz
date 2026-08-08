//! BARE METAL: Hardware Telemetry
//! Direct hardware sensor monitoring.

pub mod distortion;

/// Monitor GPU silicon diode temperature directly via OS Shell
/// Safe Fallback: Returns 0.0 if not supported/failed
/// Monitor GPU silicon diode temperature directly via OS Shell
/// Safe Fallback: Returns 0.0 if not supported/failed
pub fn check_gpu_thermal_throttle() -> f64 {
    if cfg!(any(target_os = "windows", target_os = "linux")) {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(reading_celsius) = stdout.trim().lines().next().unwrap_or("0").trim().parse::<f64>() {
                    if reading_celsius > 82.0 {
                        tracing::warn!("🔥 [Bare Metal] THERMAL ALERT: GPU Temperature at {}°C. Throttling imminent.", reading_celsius);
                    }
                    return reading_celsius;
                }
            }
        }
    }

    0.0 // Default safe return if probe fails or platform isn't supported
}
