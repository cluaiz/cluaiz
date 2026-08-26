use engines::models::GgufProber;

#[test]
fn dump_all_keys() {
    let cluaiz_dir = dirs::home_dir().unwrap().join(".cluaiz");
    let models_dir = cluaiz_dir.join("models");
    
    let qwen = models_dir.join("chat/qwen3vl_instruct-2b-gguf-q4_k_m/qwen3vl-2b-instruct-q4_k_m.gguf");
    let gemma = models_dir.join("chat/gemma4-e2b/gemma-4-E2B-it-Q4_K_M.gguf");
    
    for path in [qwen, gemma] {
        if path.exists() {
            println!("Probing ALL keys for: {}", path.display());
            if let Ok((metadata, tensor_infos, _)) = GgufProber::probe(&path) {
                let arch = metadata.get("general.architecture").map(|s| s.as_str()).unwrap_or("");
                println!("Arch: {}", arch);
                let mut keys: Vec<&String> = metadata.keys().collect();
                keys.sort();
                for key in keys {
                    println!("  Meta Key: {}", key);
                }
                
                // Also print the first 20 tensor names to see if they give clues
                println!("  -- Tensor Infos (First 20) --");
                let mut t_keys: Vec<&String> = tensor_infos.keys().collect();
                t_keys.sort();
                for t in t_keys.iter().take(20) {
                    println!("  Tensor: {}", t);
                }
            }
        }
    }
}