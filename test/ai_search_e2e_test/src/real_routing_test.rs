// BSL 1.1 — Cluaiz Technologies
//
// ai_search_e2e_test/src/real_routing_test.rs
//
// PURPOSE:
//   True E2E test. Sends raw user query via STREAMING HTTP API (stream: true).
//   Skill injection + CEL interception only happen in the streaming path.
//   Checks whether LLM emits `use extension::cluaiz-search` command.

use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::time::{Duration, Instant};

const USER_PROMPT: &str = "what is cluaiz tell me about web search cluaiz.com";
const AI_MODEL: &str = "default";
const ENGINE_BASE_URL: &str = "http://127.0.0.1:8000";

fn output_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("output")
}

fn output_file() -> std::path::PathBuf {
    output_dir().join("real_test_output.json")
}

fn save_output(value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output_dir())?;
    fs::write(output_file(), serde_json::to_string_pretty(value)?)?;
    println!("✅ Saved: {}", output_file().display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=============================================================");
    println!("🚀 [AI Search E2E] Testing REAL Routing & Prompt Injection");
    println!("=============================================================\n");
    println!("📝 Raw Prompt: \"{}\"\n", USER_PROMPT);
    println!("ℹ️  NOTE: Using stream=true (skill injection only works in streaming path)\n");

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    // STEP 0: Check Health
    println!("🔍 [Step 0] Engine health check...");
    match client.get(format!("{}/health", ENGINE_BASE_URL)).send().await {
        Ok(r) if r.status().is_success() => println!("✅ Engine ALIVE.\n"),
        _ => {
            println!("❌ Engine not running. Start: cargo run serve");
            return Ok(());
        }
    }

    // STEP 1: Send raw user request via STREAMING
    println!("🤖 [Step 1] Sending raw user request (streaming mode)...");
    let start = Instant::now();
    let chat_payload = json!({
        "model": AI_MODEL,
        "messages": [
            {
                "role": "user",
                "content": USER_PROMPT
            }
        ],
        "stream": true  // MUST be true for skill injection to work
    });

    let chat_res = client
        .post(format!("{}/v1/chat/completions", ENGINE_BASE_URL))
        .json(&chat_payload)
        .send()
        .await?;

    let mut final_text = String::new();
    let mut cel_triggered = false;
    let mut tool_result_injected = false;

    if chat_res.status().is_success() {
        // Read SSE stream line by line
        let bytes = chat_res.bytes().await?;
        let raw_body = String::from_utf8_lossy(&bytes).to_string();
        
        // Parse SSE data lines
        for line in raw_body.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                    // Normal streaming token
                    if let Some(token) = chunk
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|t| t.as_str())
                    {
                        final_text.push_str(token);
                    }
                }
            }
        }

        // Check for CEL command in the final assembled text
        if final_text.contains("use extension::cluaiz-search") {
            cel_triggered = true;
        }
        if final_text.contains("[SYSTEM INJECTION: TOOL EXECUTION RESULT]") 
            || final_text.contains("cluaiz.com") && final_text.len() > 500 
        {
            tool_result_injected = true;
        }

        let elapsed = start.elapsed();
        println!("✅ Stream complete in {:.2}s\n", elapsed.as_secs_f32());
        println!("──── 🤖 AI FINAL ANSWER ────────────────────────");
        // Print first 1000 chars only
        let preview = if final_text.len() > 1000 { &final_text[..1000] } else { &final_text };
        println!("{}", preview);
        println!("────────────────────────────────────────────────────────────\n");
    } else {
        let status = chat_res.status();
        let err_text = chat_res.text().await.unwrap_or_default();
        println!("❌ API failed with status {}: {}", status, err_text);
        final_text = format!("Error: {}", err_text);
    }

    // STEP 2: Save Diagnostic Data
    println!("💾 [Step 2] Saving diagnostic output...");
    let output = json!({
        "test_metadata": {
            "test_name": "real_routing_test_streaming",
            "user_prompt": USER_PROMPT,
            "total_elapsed_sec": start.elapsed().as_secs_f32(),
            "stream_mode": true
        },
        "diagnosis": {
            "cel_command_found_in_output": cel_triggered,
            "tool_result_injected": tool_result_injected
        },
        "ai_output": final_text
    });

    save_output(&output)?;

    println!("\n──── 📊 DIAGNOSIS ────────────────────────────────────");
    if cel_triggered {
        println!("🎉 [PASSED] LLM emitted `use extension::cluaiz-search`!");
    } else {
        println!("❌ [FAILED] LLM did NOT emit the CEL command.");
        println!("   Root cause options:");
        println!("   1. Engine did not inject SKILL.md into system prompt");
        println!("   2. LLM model is too small to follow CEL instruction format");
        println!("   3. Trigger keywords did not match — skill was not selected");
    }

    Ok(())
}
