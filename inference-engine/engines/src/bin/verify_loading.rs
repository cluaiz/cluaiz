use engines::models::registry::CoreRoster;

fn main() {
    println!("🚀 [ROSTER DIAGNOSTIC] Initiating cluaiz Registry Scan...");
    
    // We run the same function the TUI calls
    let manifests = CoreRoster::load_roster();
    
    println!("--------------------------------------------------");
    if manifests.is_empty() {
        println!("❌ [FAILURE] No models found in the registry!");
        println!("Check if path detection logic is correct for current CWD.");
    } else {
        println!("✅ [SUCCESS] Found {} models in registry.", manifests.len());
        println!("--------------------------------------------------");
        for (i, m) in manifests.iter().enumerate() {
            println!("{:02}. ID: {:<20} | Assets: {} | Repo: {}", i+1, m.id, m.assets.len(), m.huggingface_repo);
        }
    }
    println!("--------------------------------------------------");
}
