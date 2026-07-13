use cluaiz_shared::hardware::telemetry;
use std::time::Duration;

fn main() {
    println!("📡 Testing Sovereign Telemetry Singleton...");
    
    let state = telemetry::get_pulse();
    
    for i in 0..5 {
        let pulse = state.pulse.read().unwrap();
        println!("[TICK {}] CPU: {:.2}% | RAM: {:.2}GB | VRAM: {}%", 
            i,
            pulse.cpu.utilization_pct,
            pulse.ram.used_gb,
            pulse.vram_pressure_pct
        );
        drop(pulse);
        std::thread::sleep(Duration::from_millis(600));
    }
}
