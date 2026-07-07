// BSL 1.1 — Cluaiz Technologies
//
// ai_search_e2e_test/src/single_pass_e2e.rs
//
// PURPOSE:
//   True E2E test for the new Single-Pass Native Trigger Architecture.
//   Sends raw user query via STREAMING HTTP API (stream: true).
//   Logs all events sequentially (Input -> AI Think -> Trigger Intercept -> Execution -> Resume -> Final Answer).
//   Outputs a highly detailed JSON file tracking every phase.

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
    output_dir().join("single_pass_e2e_out.json")
}

fn save_output(value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output_dir())?;
    fs::write(output_file(), serde_json::to_string_pretty(value)?)?;
    println!("✅ Saved detailed trace to: {}", output_file().display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=============================================================");
    println!("🚀 [Single-Pass Native Orchestrator E2E] Deep Trace Test");
    println!("=============================================================\n");

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let chat_payload = json!({
        "model": AI_MODEL,
        "messages": [
            {
                "role": "user",
                "content": USER_PROMPT
            }
        ],
        "stream": true
    });

    let start = Instant::now();
    let chat_res = client
        .post(format!("{}/v1/chat/completions", ENGINE_BASE_URL))
        .json(&chat_payload)
        .send()
        .await?;

    let mut final_text = String::new();
    let mut trace_logs_1 = String::new();
    let mut trace_logs_5 = String::new();
    let mut matched_skill = String::new();
    let mut extracted_payload = Value::Null;
    let mut tool_output_payload = Value::Null;
    
    // Output states
    let mut current_phase = "AI_THINKING"; // AI_THINKING -> PAUSE_INTERCEPT -> RESUMED_ANSWER
    
    // Extracted logs
    let mut intercept_log = String::new();
    let mut resume_log = String::new();
    
    // Raw token buffer
    let mut token_buffer = String::new();

    if chat_res.status().is_success() {
        use futures_util::StreamExt;
        let mut stream = chat_res.bytes_stream();
        
        while let Some(chunk_res) = stream.next().await {
            if let Ok(chunk) = chunk_res {
                let chunk_str = String::from_utf8_lossy(&chunk);
                for line in chunk_str.lines() {
                    let line = line.trim();
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" { continue; }
                        
                        if let Ok(json) = serde_json::from_str::<Value>(data) {
                            if let Some(choices) = json.get("choices") {
                                if let Some(choice) = choices.get(0) {
                                    if let Some(delta) = choice.get("delta") {
                                        if let Some(content) = delta.get("content") {
                                            if let Some(token) = content.as_str() {
                                                token_buffer.push_str(token);
                                                
                                                if token_buffer.contains("⚙️ [Agentic Pause] Engine intercepted tool execution for") {
                                                    current_phase = "PAUSE_INTERCEPT";
                                                    
                                                    // Extract skill name
                                                    if let Some(start_idx) = token_buffer.find('\'') {
                                                        if let Some(end_idx) = token_buffer[start_idx+1..].find('\'') {
                                                            matched_skill = token_buffer[start_idx+1 .. start_idx+1+end_idx].to_string();
                                                        }
                                                    }
                                                    intercept_log = "⚙️ [Agentic Pause] Engine intercepted tool execution for '".to_string() + &matched_skill + "'...";
                                                    
                                                    // Parse out the JSON payload from the trace_logs_1 
                                                    if let Some(json_start) = trace_logs_1.find('{') {
                                                        if let Some(json_end) = trace_logs_1.rfind('}') {
                                                            let json_str = &trace_logs_1[json_start..=json_end];
                                                            if let Ok(v) = serde_json::from_str(json_str) {
                                                                extracted_payload = v;
                                                            }
                                                        }
                                                    }
                                                    token_buffer.clear();
                                                    continue;
                                                }
                                                
                                                if token_buffer.contains("✅ [Agentic Pause] Result injected into context window.") {
                                                    resume_log = "✅ [Agentic Pause] Result injected into context window. PIVOT_CONTINUE triggered.".to_string();
                                                    current_phase = "RESUMED_ANSWER";
                                                    
                                                    // Try extracting TOOL_OUTPUT_LOG
                                                    if let Some(start_idx) = token_buffer.find("<TOOL_OUTPUT_LOG>") {
                                                        if let Some(end_idx) = token_buffer.find("</TOOL_OUTPUT_LOG>") {
                                                            let payload_str = &token_buffer[start_idx+17 .. end_idx];
                                                            if let Ok(v) = serde_json::from_str(payload_str) {
                                                                tool_output_payload = v;
                                                            }
                                                        }
                                                    }
                                                    
                                                    token_buffer.clear();
                                                    continue;
                                                }

                                                // Clean up output if we aren't pausing
                                                if current_phase == "AI_THINKING" {
                                                    trace_logs_1.push_str(token);
                                                } else if current_phase == "RESUMED_ANSWER" {
                                                    trace_logs_5.push_str(token);
                                                    final_text.push_str(token);
                                                    
                                                    use std::io::Write;
                                                    print!("{}", token);
                                                    std::io::stdout().flush().unwrap();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f32();
        println!("\n\n─────────────────────────────────────────────────────────────");
        println!("✅ Stream finished in {:.2}s\n", elapsed);
        
        // ─── STEP 3: Create structured JSON Output ────────────────────────────────
        let summary_json = json!({
            "test_type": "SINGLE_PASS_ARCHITECTURE_E2E",
            "test_metadata": {
                "user_prompt": USER_PROMPT,
                "model": AI_MODEL,
                "description": "Engine natively intercepted <TRIGGER>, executed tool, injected payload, and AI synthesized final answer.",
                "total_elapsed_sec": elapsed
            },
            "execution_trace": {
                "1_initial_trigger_stream": trace_logs_1,
                "2_engine_interception": {
                    "log": intercept_log,
                    "matched_skill": matched_skill,
                    "extracted_payload": extracted_payload
                },
                "3_tool_execution_result": tool_output_payload.clone(),
                "4_engine_resume": {
                    "log": resume_log
                },
                "5_final_answer_stream": trace_logs_5
            },
            "search_extension_payload": tool_output_payload,
            "final_output": final_text,
            "hardware_usage": {
                "note": "Metrics logged locally by engine."
            }
        });

        save_output(&summary_json)?;
    } else {
        println!("❌ API Error: Status {}", chat_res.status());
        let err_text = chat_res.text().await.unwrap_or_default();
        println!("Details: {}", err_text);
    }

    Ok(())
}
