use color_eyre::Result;
use colored::Colorize;
use cluaiz_models::DocumentIngestor;
use onnx_engine::OnnxEngine;

pub async fn execute(file_path: &str) -> Result<()> {
    println!("\n  {} [cluaiz] Document Ingestion Pipeline Initiated", "🚀".green());
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
    
    let spin = crate::ui::spinner::Spinner::start("Loading ONNX driver…");
    let load_ok = onnx_driver.load_vision_model(&model_path_str);
    spin.stop();
    if let Err(e) = load_ok {
        eprintln!("  {} [WARNING] ONNX Vision weights not found at '{}'. Please ensure the download completed. Error: {}", "⚠️".yellow(), model_path_str, e);
        eprintln!("  {} Aborting ingestion due to missing model weights.", "🛑".red());
        return Ok(());
    }

    let spin = crate::ui::spinner::Spinner::start("Processing document…");
    let ingest_result = ingestor.ingest_and_vectorize(file_path, &onnx_driver, None, 512, None, true, &[]);
    spin.stop();
    match ingest_result {
        Ok(results) => {
            println!("  {} Successfully processed and chunked.", "✅".green());
            println!("  {} Total Semantic Chunks Generated: {}\n", "✂️ ".cyan(), results.len().to_string().yellow().bold());

            for (i, (text, vector)) in results.iter().enumerate().take(5) {
                println!("  {} {}:\n{}", "CHUNK".magenta(), (i + 1).to_string().cyan(), text.dimmed());
                
                // Show the mathematical representation (Embedding Vector)
                let vec_preview: Vec<String> = vector.iter().take(5).map(|v| format!("{:.4}", v)).collect();
                println!("  {} [{}, ...] (Total Dimensions: {})\n", "🧬 EMBEDDING VECTOR:".blue(), vec_preview.join(", "), vector.len());
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
