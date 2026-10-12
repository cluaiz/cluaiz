//! `cluaiz ps`: the model a running `cluaiz serve` has loaded.

use color_eyre::{eyre::eyre, Result};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let response = crate::cli::server::request(reqwest::Method::GET, "/v1/system/ps")
        .send()
        .await
        .map_err(|_| eyre!("no server is running at {} (start one with `cluaiz serve`)", crate::cli::server::base_url()))?;
    if !response.status().is_success() {
        return Err(eyre!("server answered {}", response.status()));
    }
    let data: serde_json::Value = response.json().await?;
    let processes = data["active_processes"].as_array().cloned().unwrap_or_default();
    if processes.is_empty() {
        println!("No model loaded.");
        return Ok(());
    }
    let header = format!("{:<40}  {:<10}  {:>5}  {:>8}  {:>10}  {}", "MODEL", "PLACEMENT", "NGL", "CONTEXT", "VRAM", "PID");
    println!("{}", header.bold());
    for p in processes {
        println!(
            "{:<40}  {:<10}  {:>5}  {:>8}  {:>10}  {}",
            p["model_id"].as_str().unwrap_or("?"),
            p["tier"].as_str().unwrap_or("?"),
            p["n_gpu_layers"].as_i64().unwrap_or(0),
            p["context_size"].as_i64().unwrap_or(0),
            format!("{:.2} GB", p["vram_gb"].as_f64().unwrap_or(0.0)),
            p["pid"].as_str().unwrap_or("?"),
        );
    }
    Ok(())
}
