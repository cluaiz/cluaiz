use color_eyre::Result;
use colored::Colorize;
use sysinfo::System;
use cluaiz_shared::hardware::governor::HardwareGovernor;

pub async fn execute() -> Result<()> {
    println!("\n  {} [cluaiz] Sovereign Process Audit...", "🔍".cyan());

    let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    let port = std::env::var("cluaiz_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(perms.api_port);
    let url = format!("{}://localhost:{}/v1/system/ps", perms.connection_protocol, port);

    let client = reqwest::Client::new(); 
    let res = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => {
            println!("  {} No active daemon found. Please run `cluaiz serve` first.", "❌".red());
            return Ok(());
        }
    };

    let data: serde_json::Value = res.json().await?;
    let active_processes = data["active_processes"].as_array().unwrap_or(&vec![]).clone();

    if active_processes.is_empty() {
        println!("  {} No active neural engines running.", "💤".yellow());
        return Ok(());
    }

    // Print table header
    println!("\n  {0:<15} | {1:<6} | {2:<12} | {3:<10} | {4:<15}", 
        "MODEL ID".bold(), 
        "PID".bold(), 
        "VRAM LOAD".bold(), 
        "CONTEXT".bold(), 
        "ENGINE".bold()
    );
    println!("  {0:-<15}-+-{0:-<6}-+-{0:-<12}-+-{0:-<10}-+-{0:-<15}", "");

    // Print rows
    for info in active_processes {
        let model_id = info["model_id"].as_str().unwrap_or("Unknown");
        let pid = info["pid"].as_str().unwrap_or("Unknown");
        let vram_gb = info["vram_gb"].as_f64().unwrap_or(0.0);
        let context_size = info["context_size"].as_i64().unwrap_or(0);
        let engine = info["engine"].as_str().unwrap_or("Unknown");

        let vram_str = format!("{:.2} GB", vram_gb);
        println!("  {0:<15} | {1:<6} | {2:<12} | {3:<10} | {4:<15}", 
            model_id.cyan(), 
            pid.yellow(), 
            vram_str.magenta(), 
            context_size.to_string().green(), 
            engine
        );
    }

    println!();
    Ok(())
}
