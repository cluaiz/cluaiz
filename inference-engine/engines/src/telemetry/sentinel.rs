//! ═══════════════════════════════════════════════════════════════════════
//!   Telemetry: Dynamic Memory Sentinel (Safety Guard)
//! ═══════════════════════════════════════════════════════════════════════

use sysinfo::System;

pub struct MemorySentinel {
    sys: System,
    buffer_pct: f64,
}

impl Default for MemorySentinel {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySentinel {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_memory();
        
        // We enforce a strict 5% safety margin on top of OS reported available memory
        // to prevent hard OOM kernel panics when large models are loaded.
        let buffer_pct = 0.05; 
        
        Self {
            sys,
            buffer_pct,
        }
    }

    /// Checks if it's safe to load a model of the given size (in bytes)
    pub fn is_safe_to_load(&mut self, required_bytes: u64) -> bool {
        self.sys.refresh_memory();
        
        let total_mem = self.sys.total_memory();
        let avail_mem = self.sys.available_memory();
        let buffer_bytes = (total_mem as f64 * self.buffer_pct) as u64;

        let safe_limit = avail_mem.saturating_sub(buffer_bytes);

        cluaiz_shared::dev_info!("🛡️ [Sentinel] Available: {}MB | Buffer: {}MB | Safe Limit: {}MB | Required: {}MB", 
            avail_mem / 1024 / 1024, buffer_bytes / 1024 / 1024, safe_limit / 1024 / 1024, required_bytes / 1024 / 1024);

        required_bytes < safe_limit
    }

    /// Emergency Kill Switch: Checks if memory is critically low (<2%)
    pub fn check_critical_state(&mut self) -> bool {
        self.sys.refresh_memory();
        let total_mem = self.sys.total_memory();
        let avail_mem = self.sys.available_memory();
        
        let critical_threshold = (total_mem as f64 * 0.02) as u64; // 2% Threshold
        
        avail_mem < critical_threshold
    }
}
