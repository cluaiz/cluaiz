use engines::models::manager::hf_hub::HuggingFaceHub;

#[tokio::main]
async fn main() {
    println!("Testing build_manifest...");
    let repo_id = "unsloth/gemma-4-26B-A4B-it-qat-GGUF";
    match HuggingFaceHub::list_variants(repo_id).await {
        Ok(variants) => {
            println!("Variants found: {}", variants.len());
            for v in &variants {
                println!("Variant: {} | filename: {} | size: {}", v.variant_id, v.filename, v.size_gb);
                match HuggingFaceHub::build_manifest(repo_id, &v.filename, v.size_gb).await {
                    Ok(m) => {
                        println!("Successfully built manifest: ID={}, name={}, category={}", m.id, m.name, m.category);
                    }
                    Err(e) => {
                        println!("Failed to build manifest: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to list variants: {}", e);
        }
    }
}
