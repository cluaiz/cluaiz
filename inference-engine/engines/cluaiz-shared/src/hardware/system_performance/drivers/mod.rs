pub mod nvidia;
pub mod amd_rocm;
pub mod apple_metal;
pub mod vulkan;
pub mod openvino;
pub mod qnn;
pub mod tpu;
pub mod ghost;
pub mod cpu_generic;

use anyhow::Result;

/// 🏛️ The Sovereign Hardware Contract: All Silicon Drivers must implement this.
/// This ensures ZERO hardcoding in the main performance engine.
pub trait HardwareDriver: Send + Sync {
    fn name(&self) -> &str;

    // --- READ OPERATIONS ---
    fn temperature_c(&self) -> Result<f32>;
    fn utilization_pct(&self) -> Result<f32>;
    fn clock_mhz(&self) -> Result<u32>;
    fn vram_used_mb(&self) -> Result<u64>;
    fn vram_total_mb(&self) -> Result<u64>;
    fn power_draw_watts(&self) -> Result<f32>;

    // --- CONTROL OPERATIONS (Mission 13) ---
    fn set_power_limit_watts(&self, _limit: u32) -> Result<()> {
        Err(anyhow::anyhow!("Power limit control not supported on this hardware"))
    }
    
    fn set_clock_mhz(&self, _mhz: u32) -> Result<()> {
        Err(anyhow::anyhow!("Clock control not supported on this hardware"))
    }
}
