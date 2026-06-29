use color_eyre::Result;
use colored::Colorize;
use engines::neural_foundry::ingestion::DocumentIngestor;
use neural_core::interfaces::router_contract::{EmbeddingDriver, EngineError};

use cluaiz_onnx::engine::OnnxEngine;

pub async fn execute(file_path: &str) -> Result<()> {
    println!("\n  {} [cluaiz] Sovereign Ingestion Pipeline Initiated", "🚀".green());
    println!("  {} Target File: {}\n", "📄".cyan(), file_path.bold());

    let ingestor = DocumentIngestor::new();
    
    // Initialize Real ONNX Engine (Path resolved dynamically via Unified Library)
    let mut onnx_driver = OnnxEngine::new()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to initialize ONNX Engine: {}", e))?;
    
    // Dynamic Resolution: Instead of hardcoding, we resolve the model directory relative to the executable/workspace
    let mut current_dir = std::env::current_dir().unwrap_or_default();
    // Assuming we are in Apps/cli or workspace root, locate models/library dynamically
    while !current_dir.join("models").exists() && current_dir.parent().is_some() {
        current_dir = current_dir.parent().unwrap().to_path_buf();
    }
    let model_path = current_dir.join("models/library/CLIP/model.onnx");
    let model_path_str = model_path.to_string_lossy();
    
    println!("  {} Loading Real ONNX Gatekeeper...", "🔮".magenta());
    if let Err(e) = onnx_driver.load_vision_model(&model_path_str, None) {
        eprintln!("  {} [WARNING] ONNX Vision weights not found at '{}'. Please ensure the download completed. Error: {}", "⚠️".yellow(), model_path_str, e);
        eprintln!("  {} Aborting ingestion due to missing brain.", "🛑".red());
        return Ok(());
    }

    match ingestor.ingest_and_vectorize(file_path, &onnx_driver) {
        Ok(results) => {
            println!("  {} Successfully processed and chunked.", "âœ…".green());
            println!("  {} Total Semantic Chunks Generated: {}\n", "âœ‚ï¸".cyan(), results.len().to_string().yellow().bold());

            for (i, (text, vector)) in results.iter().enumerate().take(5) {
                println!("  {} {}:\n{}", "CHUNK".magenta(), (i + 1).to_string().cyan(), text.dimmed());
                
                // Show the mathematical representation (Embedding Vector)
                let vec_preview: Vec<String> = vector.iter().take(5).map(|v| format!("{:.4}", v)).collect();
                println!("  {} [{}, ...] (Total Dimensions: {})\n", "🧬 MATHEMATICAL SOUL:".blue(), vec_preview.join(", "), vector.len());
            }

            if results.len() > 5 {
                println!("  ... and {} more chunks.", (results.len() - 5).to_string().magenta());
            }
        },
        Err(e) => {
            eprintln!("  {} Ingestion Failed: {}", "âŒ".red(), e);
        }
    }

    Ok(())
}
