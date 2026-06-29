use color_eyre::Result;
use colored::Colorize;
use sysinfo::System;
use cluaiz_shared::hardware::governor::HardwareGovernor;

pub async fn execute() -> Result<()> {
    println!("\n  {} [cluaiz] Sovereign Process Audit...", "🔍".cyan());

    let mut registry = HardwareGovernor::load_process_registry();
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut to_remove = Vec::new();
    let mut active_processes = Vec::new();

    // Check which processes are still alive
    for (pid_str, info) in registry.iter() {
        if let Ok(pid) = pid_str.parse::<usize>() {
            if sys.process(sysinfo::Pid::from(pid)).is_none() {
                // Process is dead
                to_remove.push(pid_str.clone());
            } else {
                active_processes.push(info.clone());
            }
        } else {
            to_remove.push(pid_str.clone());
        }
    }

    // Auto-heal stale processes
    if !to_remove.is_empty() {
        for pid in to_remove {
            registry.remove(&pid);
        }
        HardwareGovernor::save_process_registry(&registry);
    }

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
        let vram_str = format!("{:.2} GB", info.vram_gb);
        println!("  {0:<15} | {1:<6} | {2:<12} | {3:<10} | {4:<15}", 
            info.model_id.cyan(), 
            info.pid.to_string().yellow(), 
            vram_str.magenta(), 
            info.context_size.to_string().green(), 
            info.engine
        );
    }

    println!();
    Ok(())
}
