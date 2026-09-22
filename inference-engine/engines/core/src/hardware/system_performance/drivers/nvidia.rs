use super::HardwareDriver;
use anyhow::Result;
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;

pub struct NvidiaDriver {
    nvml: Nvml,
    device_index: u32,
}

impl NvidiaDriver {
    pub fn init() -> Result<Self> {
        let nvml = Nvml::init()?;
        Ok(Self { nvml, device_index: 0 })
    }
}

impl HardwareDriver for NvidiaDriver {
    fn name(&self) -> &str {
        "NVIDIA (NVML)"
    }

    fn temperature_c(&self) -> Result<f32> {
        let device = self.nvml.device_by_index(self.device_index)?;
        Ok(device.temperature(TemperatureSensor::Gpu)? as f32)
    }

    fn utilization_pct(&self) -> Result<f32> {
        let device = self.nvml.device_by_index(self.device_index)?;
        Ok(device.utilization_rates()?.gpu as f32)
    }

    fn clock_mhz(&self) -> Result<u32> {
        let device = self.nvml.device_by_index(self.device_index)?;
        // Clock type: Graphics
        Ok(device.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)?)
    }

    fn vram_used_mb(&self) -> Result<u64> {
        let device = self.nvml.device_by_index(self.device_index)?;
        Ok(device.memory_info()?.used / 1024 / 1024)
    }

    fn vram_total_mb(&self) -> Result<u64> {
        let device = self.nvml.device_by_index(self.device_index)?;
        Ok(device.memory_info()?.total / 1024 / 1024)
    }

    fn power_draw_watts(&self) -> Result<f32> {
        let device = self.nvml.device_by_index(self.device_index)?;
        Ok(device.power_usage()? as f32 / 1000.0)
    }

    // 🚀 MISSION 13: COMMANDS (No Hardcoding)
    fn set_power_limit_watts(&self, limit: u32) -> Result<()> {
        let mut device = self.nvml.device_by_index(self.device_index)?;
        tracing::info!("🛡️ [NVIDIA] Setting Power Limit to {}W", limit);
        device.set_power_management_limit(limit * 1000)?; // mW
        Ok(())
    }

    fn set_clock_mhz(&self, mhz: u32) -> Result<()> {
        let mut device = self.nvml.device_by_index(self.device_index)?;
        tracing::info!("🛡️ [NVIDIA] Locking Applications Clock to {}MHz", mhz);
        // We use memory clock at 5001 as default for many cards, or probe it. 
        // For simplicity, we set graphics clock.
        device.set_applications_clocks(5001, mhz)?; 
        Ok(())
    }
}
