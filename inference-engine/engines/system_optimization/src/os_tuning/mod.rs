//! BARE METAL: OS Tuning
//! Handles process priority and scheduling optimizations.

/// Windows Process Priority Constants
const WIN_PRIORITY_REALTIME: &str = "256";
const WIN_PRIORITY_HIGH: &str = "128";

/// Elevate process priority to ensure the AI Engine gets priority CPU scheduling
/// Priority Levels: "realtime" (256) or "high" (128)
pub fn elevate_process_priority(level: &str) -> Result<(), &'static str> {
    let pid = std::process::id();
    let priority_val = match level.to_lowercase().as_str() {
        "realtime" => WIN_PRIORITY_REALTIME,
        _ => WIN_PRIORITY_HIGH, // Default to High
    };

    tracing::info!("🚀 [Bare Metal] Attempting to elevate process priority to {} (Value: {}) for PID {}", level, priority_val, pid);

    if cfg!(target_os = "windows") {
        let output = std::process::Command::new("wmic")
            .args(["process", "where", &format!("processid={}", pid), "call", "setpriority", priority_val])
            .output()
            .map_err(|_| "Failed to execute wmic")?;

        if output.status.success() {
            tracing::info!("🚀 [Bare Metal] Windows priority elevated to {}.", level.to_uppercase());
            Ok(())
        } else {
            tracing::warn!("⚠️ [Bare Metal] Windows elevation rejected. Admin rights might be required.");
            Err("Priority elevation rejected by OS")
        }
    } else if cfg!(target_os = "linux") {
        let nice_val = if level == "realtime" { "-20" } else { "-10" };
        let output = std::process::Command::new("renice")
            .args(["-n", nice_val, "-p", &pid.to_string()])
            .output()
            .map_err(|_| "Failed to execute renice")?;

        if output.status.success() {
            tracing::info!("🚀 [Bare Metal] Linux priority elevated via renice (Nice: {}).", nice_val);
            Ok(())
        } else {
            tracing::warn!("⚠️ [Bare Metal] Linux elevation rejected.");
            Err("Priority elevation rejected by renice")
        }
    } else {
        tracing::warn!("⚠️ [Bare Metal] Process elevation not supported on this OS.");
        Err("Process elevation not supported on this architecture")
    }
}

