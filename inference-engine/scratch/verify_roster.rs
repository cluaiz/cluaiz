use engines::models::registry::NeuralRoster;

fn main() {
    let manifests = NeuralRoster::load_roster();
    println!("--- Neural Roster Verification ---");
    println!("Total models found: {}", manifests.len());
    for m in &manifests {
        println!("ID: {:<25} | Name: {:<30} | Family: {}", m.id, m.name, m.family);
    }
}
