use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType};
use engines::neural_foundry::executor::sandbox::UnifiedExecutor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("🚀 [Engine CEL Test] Testing Engine 3-Way Extension Suite");
    println!("============================================================\n");
    
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    
    // Check if Engine is alive before running tests
    match client.get("http://127.0.0.1:8000/health").send().await {
        Ok(res) if res.status().is_success() => println!("✅ Engine is ALIVE and reachable on port 8000.\n"),
        _ => {
            println!("❌ Engine is NOT running. Start the engine (`cargo run serve`) before running this test suite.");
            return Ok(());
        }
    }

    println!("------------------------------------------------------------");
    println!("🛠️  Running Test 1: HTTP Native API (/v1/execute)");
    println!("------------------------------------------------------------");
    test_http_native(&client).await?;
    
    println!("\n------------------------------------------------------------");
    println!("🛠️  Running Test 2: CEL AST Execution (/v1/cel/execute)");
    println!("------------------------------------------------------------");
    test_cel_execution(&client).await?;
    
    println!("\n------------------------------------------------------------");
    println!("🛠️  Running Test 3: C-Pointer (FFI) Native Invocation");
    println!("------------------------------------------------------------");
    test_ffi_native().await?;

    println!("\n============================================================");
    println!("🎉 All 3 layers successfully executed and returned valid JSON!");
    println!("============================================================");

    Ok(())
}

async fn test_http_native(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://127.0.0.1:8000/v1/execute/cluaiz-search/search";
    
    let payload = json!({
        "params": {
            "action": "query",
            "target": "Rust programming language",
            "context": ""
        }
    });

    let start_time = std::time::Instant::now();
    let res = client.post(url).json(&payload).send().await?;
    let elapsed = start_time.elapsed();

    println!("✅ Engine HTTP responded in {:.2}s!", elapsed.as_secs_f32());
    
    if res.status().is_success() {
        let data: Value = res.json().await?;
        if is_success_payload(&data) {
            let json_str = serde_json::to_string_pretty(&data)?;
            std::fs::create_dir_all("test/engine_cel_test/output").ok();
            std::fs::write("test/engine_cel_test/output/http_test_out.json", &json_str)?;
            println!("✅ Output is proper JSON search result!\nSaved to: test/engine_cel_test/output/http_test_out.json\n{}", json_str);
        } else {
            println!("❌ JSON is valid but not a success payload: {}", data);
            return Err("Payload mismatch".into());
        }
    } else {
        println!("❌ API Error: {}", res.status());
        return Err("HTTP Request failed".into());
    }

    Ok(())
}

async fn test_cel_execution(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://127.0.0.1:8000/v1/cel/execute";
    
    // Using a CEL script to invoke the plugin dynamically via AST parsing
    let cel_script = r#"
        execute cluaiz-search(action: "query", target: "Rust programming language", context: "")
    "#;

    let payload = json!({
        "script": cel_script
    });

    let start_time = std::time::Instant::now();
    let res = client.post(url).json(&payload).send().await?;
    let elapsed = start_time.elapsed();

    println!("✅ Engine CEL responded in {:.2}s!", elapsed.as_secs_f32());
    
    if res.status().is_success() {
        let data: Value = res.json().await?;
        // The CEL handler returns { "success": true, "result": "raw string of JSON" }
        if data["success"].as_bool() == Some(true) {
            let result_str = data["result"].as_str().unwrap_or("");
            // Parse the returned string into JSON
            if let Ok(parsed) = serde_json::from_str::<Value>(result_str) {
                if is_success_payload(&parsed) {
                    let json_str = serde_json::to_string_pretty(&parsed)?;
                    std::fs::create_dir_all("test/engine_cel_test/output").ok();
                    std::fs::write("test/engine_cel_test/output/cel_test_out.json", &json_str)?;
                    println!("✅ Output is proper JSON search result extracted from CEL!\nSaved to: test/engine_cel_test/output/cel_test_out.json\n{}", json_str);
                } else {
                    println!("❌ Parsed JSON is not a success payload: {}", parsed);
                    return Err("Payload mismatch".into());
                }
            } else {
                println!("❌ CEL result string is not valid JSON: {}", result_str);
                return Err("Not JSON".into());
            }
        } else {
            println!("❌ CEL execution failed: {:?}", data);
            return Err("CEL Failed".into());
        }
    } else {
        println!("❌ CEL API Error: {}", res.status());
        return Err("CEL HTTP Request failed".into());
    }

    Ok(())
}

async fn test_ffi_native() -> Result<(), Box<dyn std::error::Error>> {
    println!("⏳ Instantiating UnifiedExecutor locally...");
    
    let executor = UnifiedExecutor::new();
    
    let payload = json!({
        "action": "query",
        "target": "Rust programming language",
        "context": ""
    });
    
    // We must pass JSON bytes as the FFI bridge dynamically invokes it
    let json_bytes = serde_json::to_vec(&payload)?;
    
    // FFI ExtPayload requires a type and raw bytes
    let ext_payload = ExtensionPayload::new(PayloadType::Json, &json_bytes);
    
    let start_time = std::time::Instant::now();
    // Simulate what the execute_dynamic / execute_cel_plan does internally!
    match executor.execute("cluaiz-search", &ext_payload) {
        Ok(result_bytes) => {
            let elapsed = start_time.elapsed();
            println!("✅ Native C-Pointer Execution finished in {:.2}s!", elapsed.as_secs_f32());
            
            let data: Value = serde_json::from_slice(&result_bytes)?;
            if is_success_payload(&data) {
                let json_str = serde_json::to_string_pretty(&data)?;
                std::fs::create_dir_all("test/engine_cel_test/output").ok();
                std::fs::write("test/engine_cel_test/output/ffi_test_out.json", &json_str)?;
                println!("✅ Output from raw FFI bytes is proper JSON search result!\nSaved to: test/engine_cel_test/output/ffi_test_out.json\n{}", json_str);
            } else {
                println!("❌ Parsed JSON is not a success payload: {}", data);
                return Err("Payload mismatch".into());
            }
        },
        Err(e) => {
            println!("❌ UnifiedExecutor failed to invoke DLL: {}", e);
            return Err("FFI Failure".into());
        }
    }

    Ok(())
}

// Helper to verify that the payload looks exactly like the correct Search JSON structure
fn is_success_payload(payload: &Value) -> bool {
    // Let's deeply search for "results" array
    if let Some(results) = payload.get("results").and_then(|r| r.as_array()) {
        return !results.is_empty();
    }
    
    // If it's wrapped
    if let Some(nested) = payload.get("result") {
        if let Some(results) = nested.get("results").and_then(|r| r.as_array()) {
            return !results.is_empty();
        }
    }
    
    false
}
