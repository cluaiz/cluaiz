use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

pub struct PerformanceLogger;

impl PerformanceLogger {
    pub fn log_benchmark(model: &str, tps: f64, memory_mb: u64) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = Utc::now().to_rfc3339();
        let log_entry = format!(
            "[{}] MODEL: {} | SPEED: {:.2} TPS | MEMORY: {} MB\n",
            timestamp, model, tps, memory_mb
        );

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("performance_report.txt")?;

        file.write_all(log_entry.as_bytes())?;
        
        println!("🚀 [PERFORMANCE LOGGED] {} -> {:.2} TPS", model, tps);
        Ok(())
    }
}
