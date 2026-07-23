use clap::{Parser, Subcommand};
use reqwest::Client;
use std::process::exit;
use std::time::Duration;

#[derive(Parser)]
pub struct StorageCli {
    #[command(subcommand)]
    pub command: StorageCommand,
}

#[derive(Subcommand)]
pub enum StorageCommand {
    /// Show current temporary media storage usage
    TempStatus,
    /// Clean all temporary media storage
    TempClean,
}

pub async fn handle_storage_command(cli: StorageCli, api_port: u16) {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let base_url = format!("http://localhost:{}", api_port);

    match cli.command {
        StorageCommand::TempStatus => {
            let url = format!("{}/v1/system/storage/temp_media", base_url);
            match client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(data) = response.json::<serde_json::Value>().await {
                            println!("Temporary Media Storage Status:");
                            println!("Total Files: {}", data["file_count"]);
                            println!("Total Size: {}", data["total_size_mb"].as_str().unwrap_or("0 MB"));
                        } else {
                            println!("Error: Failed to parse response from engine.");
                        }
                    } else {
                        println!("Error: Engine returned status {}", response.status());
                    }
                }
                Err(e) => {
                    println!("Error: Could not connect to engine ({}). Is it running?", e);
                    exit(1);
                }
            }
        }
        StorageCommand::TempClean => {
            let url = format!("{}/v1/system/storage/temp_media/clean", base_url);
            match client.post(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        println!("Temporary media storage cleaned successfully.");
                    } else {
                        println!("Error: Engine returned status {}", response.status());
                    }
                }
                Err(e) => {
                    println!("Error: Could not connect to engine ({}). Is it running?", e);
                    exit(1);
                }
            }
        }
    }
}
