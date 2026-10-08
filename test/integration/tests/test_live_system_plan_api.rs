use std::path::Path;
use serde_json::json;

#[tokio::test]
async fn test_live_server_system_plan_all_models() {
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:8080/v1/system/plan";

    let installed_models = vec![
        "bonsai-4b-gguf-q1_0",
        "bonsai-27b-gguf-q1_0",
        "bonsai1-8b",
        "gemma-4-26b-a4b-it-qat-gguf-UD-Q4_K_XL",
        "gemma-4-e4b-it-gguf-Q4_K_M",
        "llama_3.2_instruct-3b-gguf-q4_k_m",
        "qwen3-4b-gguf-Q4_K_M",
    ];

    let mut output_log = String::new();
    output_log.push_str("================================================================================\n");
    output_log.push_str("LIVE HTTP API TEST: http://127.0.0.1:8080/v1/system/plan\n");
    output_log.push_str("================================================================================\n\n");

    println!("\n🚀 Connecting to live running daemon: {}", url);

    for model_id in &installed_models {
        println!("\n📡 [HTTP POST] Sending plan request for model: {}", model_id);
        
        let payload = json!({
            "model": model_id,
            "batch_tokens": 1
        });

        let resp = client
            .post(url)
            .json(&payload)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to connect to API server at {}. Is it running? Error: {}", url, e));

        assert!(resp.status().is_success(), "HTTP status must be success for installed model {}, got: {}", model_id, resp.status());

        let json_body: serde_json::Value = resp
            .json()
            .await
            .expect("Failed to parse response body as JSON");

        // Verify status field
        let status = json_body.get("status").and_then(|s| s.as_str()).unwrap_or("");
        assert_eq!(status, "success", "Response status must be 'success' for {}", model_id);

        let data = json_body.get("data").expect("Response must contain 'data' object");
        let winner_id = data.get("winner_candidate_id").and_then(|w| w.as_str()).unwrap_or("");
        let winner_tps = data.get("winner_generation_tps").and_then(|t| t.as_f64()).unwrap_or(0.0);
        let winner_ttft = data.get("winner_ttft_ms").and_then(|t| t.as_f64()).unwrap_or(0.0);
        let arch = data.get("architecture").and_then(|a| a.as_str()).unwrap_or("unknown");
        let candidates = data.get("candidates").and_then(|c| c.as_array()).expect("candidates array missing");

        println!("  ✅ Architecture: {}", arch);
        println!("  ✅ Winner: {} | TPS: {:.2} | TTFT: {:.2}ms", winner_id, winner_tps, winner_ttft);

        // 🛡️ Banned 0-TPS winner
        assert_ne!(winner_id, "none", "Winner cannot be none for installed model {}", model_id);
        assert!(winner_tps > 0.0, "Winner generation TPS must be strictly > 0.0 for {}", model_id);
        assert!(winner_ttft > 0.0, "Winner TTFT must be strictly > 0.0 for {}", model_id);

        output_log.push_str(&format!("### MODEL: {}\n", model_id));
        output_log.push_str(&format!("  Architecture: {}\n", arch));
        output_log.push_str(&format!("  Winner Candidate: {}\n", winner_id));
        output_log.push_str(&format!("  Winner Generation TPS: {:.2}\n", winner_tps));
        output_log.push_str(&format!("  Winner TTFT ms: {:.2}\n", winner_ttft));
        output_log.push_str("--------------------------------------------------------------------------------\n");
        output_log.push_str("CANDIDATES:\n");
        for c in candidates {
            let cid = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let feasible = c.get("feasible").and_then(|v| v.as_bool()).unwrap_or(false);
            let tps = c.get("generation_tps").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let vram = c.get("vram_required_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let ram = c.get("ram_required_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let is_w = c.get("is_winner").and_then(|v| v.as_bool()).unwrap_or(false);
            let reason = c.get("ineligibility_reason").and_then(|v| v.as_str()).unwrap_or("");

            output_log.push_str(&format!(
                "  [{}] feasible={} | vram={:.2}GB | ram={:.2}GB | tps={:.2} | is_winner={}{}\n",
                cid, feasible, vram, ram, tps, is_w,
                if !reason.is_empty() { format!(" | reason: {}", reason) } else { String::new() }
            ));
        }
        output_log.push_str("\nFULL JSON:\n");
        output_log.push_str(&serde_json::to_string_pretty(&json_body).unwrap());
        output_log.push_str("\n\n================================================================================\n\n");
    }

    // Uninstalled Model Test
    let uninstalled_id = "qwen2.5-7b-instruct-q4_k_m";
    println!("\n📡 [HTTP POST] Testing Uninstalled Model: {}", uninstalled_id);
    let payload = json!({
        "model": uninstalled_id,
        "batch_tokens": 1
    });

    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| panic!("Failed to connect to API server: {}", e));

    let json_body: serde_json::Value = resp.json().await.unwrap();
    let status = json_body.get("status").and_then(|s| s.as_str()).unwrap_or("");
    let err_msg = json_body.get("message").or_else(|| json_body.get("error")).and_then(|s| s.as_str()).unwrap_or("");

    println!("  ✅ Uninstalled Response Status: {} | Message: {}", status, err_msg);
    assert_eq!(status, "error", "Uninstalled model must return status 'error'");

    output_log.push_str(&format!("### UNINSTALLED MODEL: {}\n", uninstalled_id));
    output_log.push_str(&format!("  Status: {}\n", status));
    output_log.push_str(&format!("  Error Message: {}\n", err_msg));
    output_log.push_str("================================================================================\n");

    let dump_path = Path::new("c:/Users/Aryan/my/Cluaiz-workspace/live_model_plans_test_output.txt");
    std::fs::write(dump_path, &output_log).expect("Failed to write output text file");
    println!("\n📄 Saved complete live API test results to: {:?}", dump_path);
}
