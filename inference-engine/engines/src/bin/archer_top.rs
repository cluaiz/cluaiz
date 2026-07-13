//! cluaiz-top: The cluaiz Neural Pulse Monitor (CLI).
//! High-speed, terminal-native monitoring for the cluaiz Engine.

use cluaiz_shared::hardware::telemetry;
use std::io::{stdout, Write};
use std::thread;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // 🏛️ Initialize the cluaiz Governor (Triggers Calibration if JSON is missing)
    let _governor = cluaiz_shared::HardwareGovernor::start();
    
    let sensor = telemetry::get_pulse();
    let mut stdout = stdout();

    println!("\x1B[2J\x1B[H"); // Clear screen
    println!("🧿 cluaiz Neural PULSE MONITOR V1.0 - [STEALTH MODE ACTIVE]");
    println!("══════════════════════════════════════════════════════════");

    loop {
        // 1. Data Collection
        let pulse = sensor.pulse.read().unwrap();
        let per_core_readings = pulse.per_core_usage.clone();
        let vram_usage = pulse.vram_pressure_pct;
        let reading_celsius = pulse.cpu.temperature_c;

        // 2. Render UI (ANSI Express)
        print!("\x1B[H"); // Move to top
        println!("🧿 cluaiz Neural PULSE MONITOR V1.0 - Hardware Pulse Target: LOCAL");
        println!("══════════════════════════════════════════════════════════");

        // CPU Grid (Per-Core Audit)
        println!("\n[CPU CORE AUDIT]");
        for (core_index, usage_reading) in per_core_readings.iter().enumerate() {
            let usage_bar = render_bar(*usage_reading, 20);
            print!(
                " Core {:02} [{}] {:.1}%   ",
                core_index, usage_bar, *usage_reading as f32
            );
            if (core_index + 1) % 2 == 0 {
                println!();
            }
        }

        // VRAM & Global Sensors
        println!("\n\n[Hardware DIODES]");
        let vram_bar = render_bar(vram_usage, 40);
        println!(" VRAM Pressure: [{}] {}%", vram_bar, vram_usage);
        println!(" CPU Thermal:   {:.1}°C", reading_celsius);

        // Core Metrics
        println!("\n[Core KERNEL METRICS]");
        println!(" Relay Latency:  {} ms", pulse.relay_latency_ms);
        println!(" context Cache:  {} MB", pulse.kv_cache_footprint_mb);
        println!(" Disk Load:      {} MB/s", pulse.storage_throughput_mbps);

        println!("\n══════════════════════════════════════════════════════════");
        println!(" [Ctrl+C] to exit 'Stealth Mode'");

        stdout.flush()?;
        thread::sleep(Duration::from_millis(250));
    }
}

fn render_bar(percentage: u32, bar_length: usize) -> String {
    let filled_segments = (percentage as f64 / 100.0 * bar_length as f64).round() as usize;
    let mut bar_render = String::with_capacity(bar_length);
    for segment_index in 0..bar_length {
        if segment_index < filled_segments {
            bar_render.push('█');
        } else {
            bar_render.push('░');
        }
    }
    bar_render
}
