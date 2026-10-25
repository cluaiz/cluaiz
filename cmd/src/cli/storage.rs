use clap::{Parser, Subcommand};
use std::process::exit;

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

pub async fn handle_storage_command(cli: StorageCli) {
    let method = match cli.command {
        StorageCommand::TempStatus => "storage.status",
        StorageCommand::TempClean => "storage.clean",
    };
    match crate::cli::server::cep(method, serde_json::json!({})).await {
        Ok(data) => match cli.command {
            StorageCommand::TempStatus => {
                println!("Temporary Media Storage Status:");
                println!("Total Files: {}", data["file_count"]);
                println!(
                    "Total Size: {}",
                    data["total_size_mb"].as_str().unwrap_or("0 MB")
                );
            }
            StorageCommand::TempClean => println!(
                "Temporary media storage cleaned: {} files, {} bytes.",
                data["removed_files"], data["freed_bytes"]
            ),
        },
        Err(error) => {
            eprintln!("Error: {error}");
            exit(1);
        }
    }
}
