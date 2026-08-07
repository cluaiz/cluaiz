use engines::models::registry::CoreRoster;

fn main() {
    let manifests = CoreRoster::load_roster();
    println!("--- Audio / TTS / STT Roster Models ---");
    for m in &manifests {
        if m.category == "audio" || m.has_audio {
            println!("ID: {:<30} | HF Repo: {:<35} | Category: {}", m.id, m.huggingface_repo, m.category);
        }
    }
}
