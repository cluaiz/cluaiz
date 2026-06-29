use engines::memory::embedding_generator::EmbeddingGenerator;
use engines::memory::storage_bridge::load_storage_bridge;
use engines::neural_foundry::security::permission_schema::PermissionSchema;

fn main() {
    println!("🧪 [Test Bridge] Starting Diagnostic...");

    // 1. Load Permissions Schema
    let schema = PermissionSchema::load();
    println!("⚙️ [Test Bridge] PermissionSchema loaded:");
    println!("  - vectorize_user_input: {:?}", schema.vectorize_user_input);
    println!("  - vectorize_ai_response: {:?}", schema.vectorize_ai_response);

    // 2. Generate vector embedding
    let text = "Hello cluaiz Database!";
    println!("⚙️ [Test Bridge] Generating vector embedding for: '{}'...", text);
    let vector = EmbeddingGenerator::generate_vector(text);
    println!("✅ [Test Bridge] Generated 16-D Vector: {:?}", vector);

    // 3. Perform direct reqwest POST to cluaizdb
    let url = "http://127.0.0.1:9090/neuron";
    println!("⚙️ [Test Bridge] Performing direct POST to {}...", url);
    let client = reqwest::blocking::Client::new();
    let creator_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string();
    let body = serde_json::json!({
        "raw_payload": text,
        "vector_data": vector,
        "model_creator_hash": creator_hash,
        "payload_type": "text",
        "dna": null,
        "adjacency": null
    });

    match client.post(url)
        .header("x-tenant-id", "default_sandbox")
        .json(&body)
        .send()
    {
        Ok(res) => {
            println!("  - Status Code: {}", res.status());
            if let Ok(body_text) = res.text() {
                println!("  - Response Body: {}", body_text);
            }
        }
        Err(e) => {
            println!("❌ [Test Bridge] POST failed: {:?}", e);
        }
    }

    println!("🧪 [Test Bridge] Diagnostic complete!");
}
