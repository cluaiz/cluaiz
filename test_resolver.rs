use std::path::PathBuf;

fn resolve_active_model_path() -> Option<PathBuf> {
    let hub_path = std::path::PathBuf::from(r"C:\Users\Aryan\.cluaiz");
    let perm_path = hub_path.join("engine").join("config").join("Permission.json");
    let perm_str = std::fs::read_to_string(perm_path).ok()?;
    let perm_json: serde_json::Value = serde_json::from_str(&perm_str).ok()?;
    let active_id = perm_json
        .get("chat_models")?
        .get("text")?
        .as_str()?
        .replace(':', "-");
    
    println!("active_id: {}", active_id);
    let models_root = hub_path.join("models");
    let categories = ["chat", "embedding", "vision", "audio", "code"];
    for category in &categories {
        let model_dir = models_root.join(category).join(&active_id);
        println!("Checking dir: {}", model_dir.display());
        if model_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&model_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    println!("Found file: {}", p.display());
                    if p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

fn main() {
    let p = resolve_active_model_path();
    println!("Resolved path: {:?}", p);
}
