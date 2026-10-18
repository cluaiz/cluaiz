//! Step logs for the multi-stage TTS families. They go through the engine logger at debug level
//! (earlier versions printed to the terminal and wrote `log.txt` into the working directory).
//! Callers never pass the text being spoken, only counts and shapes.

/// Kept for callers that mark the start of a run; the engine logger needs no setup.
pub fn init_new_run_log() {}

/// Log one execution step of a TTS family.
pub fn log_step(family: &str, stage: &str, details: &str) {
    tracing::debug!("🎙️ [{family}] [{stage}] {details}");
}
