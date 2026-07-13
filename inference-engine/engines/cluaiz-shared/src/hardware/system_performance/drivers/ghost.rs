use super::HardwareDriver;
use anyhow::Result;

/// 👻 Ghost Simulation Driver
/// Generates virtual hardware metrics for testing and headless modes.
pub struct GhostDriver;

impl GhostDriver {
    pub fn init() -> Result<Self> { Ok(Self) }
}

impl HardwareDriver for GhostDriver {
    fn name(&self) -> &str { "Ghost (Simulation)" }

    fn temperature_c(&self) -> Result<f32> { Ok(35.5) }
    fn utilization_pct(&self) -> Result<f32> { Ok(10.0) }
    fn clock_mhz(&self) -> Result<u32> { Ok(1200) }
    fn vram_used_mb(&self) -> Result<u64> { Ok(512) }
    fn vram_total_mb(&self) -> Result<u64> { Ok(8192) }
    fn power_draw_watts(&self) -> Result<f32> { Ok(15.0) }
}
