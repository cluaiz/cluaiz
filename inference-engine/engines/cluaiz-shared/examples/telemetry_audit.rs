use cluaiz_shared::hardware::telemetry;
use std::time::Duration;

fn main() {
    println!("📡 Sovereign Telemetry Audit: REAL-TIME VERIFICATION");
    println!("--------------------------------------------------");
    
    // Get the singleton pulse
    let state = telemetry::get_pulse();
    
    println!("Observer started. Streaming metrics for 10 seconds...\n");

    for i in 1..=20 {
        let pulse = state.pulse.read().unwrap();
        println!(
            "[{:>2}/20] 🌡️ CPU: {:>2.0}°C ({:>2.0}%) | 🎮 GPU: {:>2.0}°C ({:>2.0}%) | 🧠 RAM: {:>4.1}GB | ⚡ TPS: {:.1}",
            i,
            pulse.cpu.temperature_c,
            pulse.cpu.utilization_pct,
            pulse.gpus.first().map(|g| g.temperature_c).unwrap_or(0.0),
            pulse.gpus.first().map(|g| g.utilization_pct).unwrap_or(0.0),
            pulse.ram.used_gb,
            pulse.relay_latency_ms as f64 / 10.0
        );
        drop(pulse);
        std::thread::sleep(Duration::from_millis(500));
    }
    
    println!("\n✅ Audit Complete. If the numbers above changed, the system is LIVE.");
}
