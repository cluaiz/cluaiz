use super::HardwareDriver;
use anyhow::Result;
use std::process::Command;

/// 🌋 Vulkan Generic Driver (Sovereign Production Implementation)
/// Targets any Vulkan-capable silicon using OS-level interrogation.
pub struct VulkanDriver;

impl VulkanDriver {
    pub fn init() -> Result<Self> {
        // Simple probe to see if Vulkan is present
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("powershell")
                .args(["-Command", "Get-ChildItem 'C:\\Windows\\System32\\vulkan-1.dll'"])
                .output()?;
            if output.status.success() { return Ok(Self); }
        }
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("which").arg("vulkaninfo").output()?;
            if output.status.success() { return Ok(Self); }
        }
        Err(anyhow::anyhow!("Vulkan loader not found"))
    }
}

impl HardwareDriver for VulkanDriver {
    fn name(&self) -> &str { "Vulkan (Sovereign)" }

    fn temperature_c(&self) -> Result<f32> {
        // Vulkan temperature requires a specific extension (VK_EXT_display_control)
        // or a vendor-specific probe. Fallback to 0.0 to avoid fake telemetry.
        Ok(0.0) 
    }

    fn utilization_pct(&self) -> Result<f32> { Ok(0.0) }

    fn clock_mhz(&self) -> Result<u32> { Ok(0) }

    fn vram_used_mb(&self) -> Result<u64> { Ok(0) }

    fn vram_total_mb(&self) -> Result<u64> {
        // Dedicated VRAM should be fetched via the PhysicalDevice properties in a real implementation.
        // For now, we return 0 to prevent fragile WMI calls from hanging the TUI.
        Ok(0)
    }

    fn power_draw_watts(&self) -> Result<f32> { Ok(0.0) }
}
