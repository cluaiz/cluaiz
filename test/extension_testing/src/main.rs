use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("🚀 [Cluaiz Test] End-to-End Extension Test: cluaiz-search");
    println!("============================================================\n");
    
    let client = Client::builder()
        .timeout(Duration::from_secs(180)) 
        .build()?;
    
    // 1. Check if Engine is alive
    println!("🔍 [Step 1] Checking Engine Health...");
    match client.get("http://127.0.0.1:8000/health").send().await {
        Ok(res) if res.status().is_success() => println!("✅ Engine is ALIVE and reachable on port 8000.\n"),
        _ => {
            println!("❌ Engine is NOT running. Please start the engine (`cluaiz serve`).");
            return Ok(());
        }
    }

    // 2. Execute the Search Extension natively via Execute Dynamic
    println!("🌐 [Step 2] Executing Native Web Search via Extension API...");
    let cel_url = "http://127.0.0.1:8000/v1/execute/cluaiz-search/search";
    
    let cel_payload = json!({
        "params": {
            "action": "query",
            "target": "Who is the current CEO of OpenAI?"
        }
    });

    println!("   Sending Execution Payload to cluaiz-search...");
    
    let start_search = std::time::Instant::now();
    let search_res = client.post(cel_url).json(&cel_payload).send().await?;
    let search_elapsed = start_search.elapsed();

    let mut search_json_result = String::new();

    if search_res.status().is_success() {
        let data: Value = search_res.json().await?;
        println!("✅ Search Completed in {:.2}s!\n", search_elapsed.as_secs_f32());
        
        println!("==================== EXTENSION JSON RESPONSE ====================");
        let pretty_json = serde_json::to_string_pretty(&data)?;
        println!("{}", pretty_json);
        println!("=================================================================\n");

        if let Some(res) = data.get("result").and_then(|r| r.as_str()) {
            search_json_result = res.to_string();
        } else {
            search_json_result = pretty_json;
        }
    } else {
        println!("❌ [Error] Extension execution failed: {}", search_res.status());
        let err_text = search_res.text().await?;
        println!("Details: {}", err_text);
        return Ok(());
    }

    // 3. Send the JSON to the AI Model to Summarize
    let chat_url = "http://127.0.0.1:8000/v1/chat/completions";
    println!("🧠 [Step 3] Passing Search Data to AI Model for Synthesis...");
    
    let chat_payload = json!({
        "model": "default",
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful AI assistant. You have just performed a web search. Read the following JSON search results and answer the user's question accurately."
            },
            {
                "role": "user",
                "content": format!("Question: Who is the current CEO of OpenAI?\n\nSearch Results:\n{}", search_json_result)
            }
        ],
        "stream": false
    });
    
    let start_chat = std::time::Instant::now();
    let chat_res = client.post(chat_url).json(&chat_payload).send().await?;
    let chat_elapsed = start_chat.elapsed();
    
    if chat_res.status().is_success() {
        let data: Value = chat_res.json().await?;
        println!("✅ AI Response Received in {:.2}s!\n", chat_elapsed.as_secs_f32());
        
        if let Some(choices) = data.get("choices") {
            if let Some(choice) = choices.as_array().and_then(|c| c.get(0)) {
                if let Some(message) = choice.get("message") {
                    if let Some(content) = message.get("content") {
                        println!("🤖 [AI Final Output]:\n{}", content.as_str().unwrap_or(""));
                    }
                }
            }
        }
    } else {
        println!("❌ [Error] Model chat failed: {}", chat_res.status());
    }
    
    println!("\n✅ Test Suite Execution Finished.");
    Ok(())
}
