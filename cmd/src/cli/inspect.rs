use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;

pub async fn execute(model_id: &str) -> Result<()> {
    println!("\n  {} [Probe] Initiating raw SSD header inspection for: {}", "⚡".cyan(), model_id.yellow().bold());

    let roster = CoreRoster::load_roster();
    let manifest = roster.into_iter().find(|m| 
        m.id.to_lowercase() == model_id.to_lowercase() ||
        m.huggingface_filename.to_lowercase() == model_id.to_lowercase() ||
        m.name.to_lowercase() == model_id.to_lowercase() ||
        m.id.replace(":", "-").to_lowercase() == model_id.to_lowercase()
    );

    if let Some(manifest) = manifest {
        if let Some(local_path) = manifest.local_path {
            let model_file = std::path::Path::new(&local_path).join(&manifest.huggingface_filename);
            if model_file.exists() {
                if manifest.huggingface_filename.ends_with(".gguf") {
                    println!("  {} [Probe] Directly analyzing GGUF binary from: {:?}", "🔍".cyan(), model_file);
                    
                    // Directly use engines::models::GgufProber
                    if let Ok((metadata, tensor_infos, tensor_count)) = engines::models::GgufProber::probe(&model_file) {
                        println!("\n  {} === GGUF HEADER RAW DATA ===", "📄".cyan());
                        println!("  - Tensor Count: {}", tensor_count);
                        println!("\n  [Metadata KVs]");
                        for (k, v) in metadata.iter() {
                            println!("    {}: {}", k.green(), v);
                        }
                        println!("\n  [Tensors (First 10 of {})]", tensor_infos.len());
                        for (i, (name, dims)) in tensor_infos.iter().enumerate().take(10) {
                            println!("    {}: {:?}", name.cyan(), dims);
                        }
                        if tensor_infos.len() > 10 {
                            println!("    ... ({} more omitted for brevity)", tensor_infos.len() - 10);
                        }
                        println!("\n  {} Inspection complete.\n", "✅".green());
                        return Ok(());
                    } else {
                        color_eyre::eyre::bail!("Failed to parse GGUF file header.");
                    }
                } else {
                    println!("  {} [Probe] Model is not a GGUF file (format: {}). Raw probing is specialized for GGUF currently.\n", "⚠️".yellow(), manifest.huggingface_filename);
                    return Ok(());
                }
            } else {
                color_eyre::eyre::bail!("Model file not found at {:?}", model_file);
            }
        } else {
            color_eyre::eyre::bail!("Model manifest does not have a local path.");
        }
    } else {
        color_eyre::eyre::bail!("Model '{}' not found in vault. Did you pull it?", model_id);
    }
}
