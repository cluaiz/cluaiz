 use super::drivers::HardwareDriver;
use anyhow::Result;

pub struct ThermalGuard {
    critical_temp: f32,
    safe_temp: f32,
    is_throttled: bool,
}

impl ThermalGuard {
    pub fn new(critical_temp: f32, safe_temp: f32) -> Self {
        Self {
            critical_temp,
            safe_temp,
            is_throttled: false,
        }
    }

    pub fn tick(&mut self, driver: &dyn HardwareDriver) -> Result<()> {
        let current_temp = driver.temperature_c()?;
        self.check_raw(current_temp as f64);
        
        if self.is_throttled {
            driver.set_power_limit_watts(100)?; // Emergency Low Power
        } else {
            // Power limit reset would go here, but we let governor manage it.
        }
        
        Ok(())
    }

    pub fn check_raw(&mut self, current_temp: f64) {
        if current_temp > self.critical_temp as f64 && !self.is_throttled {
            tracing::warn!("🔥 [ThermalGuard] CRITICAL TEMP DETECTED ({:.1}°C). Throttling imminent.", current_temp);
            self.is_throttled = true;
        } else if current_temp < self.safe_temp as f64 && self.is_throttled {
            tracing::info!("🟢 [ThermalGuard] Temperature stabilized ({:.1}°C). Releasing throttle.", current_temp);
            self.is_throttled = false;
        }
    }
}
