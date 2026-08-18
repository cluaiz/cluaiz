use engines::models::HuggingFaceHub;

#[tokio::main]
async fn main() {
    println!("Testing build_manifest...");
    let repo_id = "unsloth/gemma-4-26B-A4B-it-qat-GGUF";
    match HuggingFaceHub::list_variants(repo_id).await {
        Ok(variants) => {
            println!("Variants found: {}", variants.len());
            for v in &variants {
                println!("Variant: {} | filename: {} | size: {}", v.variant_id, v.filename, v.size_gb);
                let m = HuggingFaceHub::build_manifest(repo_id, v, None);
                println!("Successfully built manifest: ID={}, name={}, category={}", m.id, m.name, m.category);
            }
        }
        Err(e) => {
            println!("Failed to list variants: {}", e);
        }
    }
}
