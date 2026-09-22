use std::sync::atomic::{AtomicBool, Ordering};

/// 🛡️ NeuralCircuitBreaker: Prevents kernel panics and silent hangs.
/// OxiBonsai Strategy: Trips the circuit if inference becomes unstable.
pub struct NeuralCircuitBreaker {
    pub tripped: AtomicBool,
    pub last_fault: Option<String>,
    pub threshold: usize,
    pub failure_count: std::sync::atomic::AtomicUsize,
}

impl Default for NeuralCircuitBreaker {
    fn default() -> Self {
        Self {
            tripped: AtomicBool::new(false),
            last_fault: None,
            threshold: 3, // Trip after 3 consecutive failures
            failure_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl NeuralCircuitBreaker {
    pub fn new(threshold: usize) -> Self {
        Self {
            tripped: AtomicBool::new(false),
            last_fault: None,
            threshold,
            failure_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 🔬 Monitor: Checks if the system should allow inference.
    pub fn can_proceed(&self) -> bool {
        !self.tripped.load(Ordering::Relaxed)
    }

    /// 🚨 Trip: Manually shut down the circuit due to a critical error.
    pub fn trip(&mut self, reason: &str) {
        self.tripped.store(true, Ordering::SeqCst);
        self.last_fault = Some(reason.to_string());
        tracing::error!("🚨 [Circuit Breaker] TRIPPED! Reason: {}", reason);
    }

    /// 🛠️ Reset: Attempt to recover the circuit.
    pub fn reset(&mut self) {
        self.tripped.store(false, Ordering::SeqCst);
        self.failure_count.store(0, Ordering::SeqCst);
        self.last_fault = None;
        tracing::info!("🛡️ [Circuit Breaker] Reset successful. Neural paths restored.");
    }

    /// 📈 Record Failure: Increment failure count and trip if threshold reached.
    pub fn record_failure(&mut self, error: &str) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= self.threshold {
            self.trip(&format!("Failure threshold exceeded ({}): {}", count, error));
        }
    }

    /// ✅ Record Success: Reset the failure counter on successful inference.
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
    }
}
