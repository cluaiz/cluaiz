use anyhow::Result;
use cluaiz_shared::hardware::resource_negotiator::{
    negotiate_resource, EngineType, InferenceMode, ResourceRequest, PlacementTier
};
use cluaiz_shared::hardware::governor::HardwareGovernor;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use reqwest::Client;
use serde_json::json;

const BASE_URL: &str = "http://127.0.0.1:8000";

#[tokio::test]
async fn test_gemma_hardware_allocation_trace() -> Result<()> {
    println!("🧪 [Test] Running MoE Hardware Allocation Trace...");

    // Create the test output folder if it doesn't exist
    std::fs::create_dir_all("test")?;
    let log_path = "test/allocation_trace_output.txt";
    let mut file = File::create(log_path)?;

    let mut log_buf = Vec::new();
    writeln!(log_buf, "=============================================================")?;
    writeln!(log_buf, "🧬 CLUAIZ RESOURCE ALLOCATION & HARDWARE PLACEMENT DIAGNOSTIC")?;
    writeln!(log_buf, "=============================================================\n")?;

    // 1. Setup the real negotiator request for Gemma
    let request = ResourceRequest {
        engine_type: EngineType::GGUF,
        inference_mode: InferenceMode::Chat,
        model_size_gb: 13.5,
        model_path: PathBuf::from("gemma-4-26b-a4b-it-qat-gguf-UD-Q4_K_XL"),
    };

    // 2. Call the ACTUAL negotiator production logic directly!
    let grant = cluaiz_shared::hardware::resource_negotiator::negotiate_resource(&request)?;

    writeln!(log_buf, "DEBUG - Config Path: {:?}", cluaiz_shared::environment::config_manager::ConfigManager::config_dir())?;
    
    writeln!(log_buf, "--- [NEGOTIATOR PRODUCTION DECISION] ---")?;
    writeln!(log_buf, "Assigned Placement Tier: {:?}", grant.tier)?;
    writeln!(log_buf, "GPU Layers to Offload:   {}", grant.n_gpu_layers)?;
    writeln!(log_buf, "VRAM Budget Allocated:   {:.2} GB", grant.vram_budget_gb)?;
    writeln!(log_buf, "System RAM Allocated:    {:.2} GB", grant.ram_budget_gb)?;
    writeln!(log_buf, "Safety Buffer Reserved:  {:.2} GB", grant.safety_buffer_gb)?;
    writeln!(log_buf, "Expert Cache Budget:     {:.2} GB", grant.expert_cache_budget_gb)?;

    if grant.tier == PlacementTier::Hybrid || grant.tier == PlacementTier::GpuOnly {
        writeln!(log_buf, "\nStatus: Dynamic allocation succeeded. No SSD Streaming/thrashing required!")?;
    } else {
        writeln!(log_buf, "\nStatus: Model limits exceeded standard bounds or safety floor active.")?;
    }

    file.write_all(&log_buf)?;
    println!("✅ Allocation trace output written to: {}", log_path);
    
    // Print summary to console
    println!("{}", String::from_utf8_lossy(&log_buf));
    
    // ADD REAL ASSERTIONS to ensure negotiator behaves properly
    assert!(grant.vram_budget_gb >= 0.0, "VRAM budget cannot be negative");
    assert!(grant.ram_budget_gb >= 0.0, "RAM budget cannot be negative");
    assert!(grant.safety_buffer_gb >= 0.25, "Safety buffer should be at least the absolute minimum");
    
    // Since 13.5 GB model might fit entirely in VRAM (on good GPUs) or be split
    assert!(
        grant.tier == PlacementTier::CpuOnly ||
        grant.tier == PlacementTier::Hybrid || 
        grant.tier == PlacementTier::GpuOnly,
        "Placement Tier must be a valid standard tier for a standard dense model"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "Requires Cluaiz API server to be running on 127.0.0.1:8000"]
async fn test_multimodal_stream_api() -> Result<()> {
    println!("🚀 [Test] Starting Multimodal completions stream API test...");

    let client = Client::new();
    let payload = json!({
      "messages": [
        {
          "role": "system",
          "content": "You are a helpful assistant."
        },
        {
          "role": "user",
          "content": [
            {
              "type": "text",
              "text": "Describe this image in detail."
            },
            {
              "type": "image_url",
              "image_url": {
                "url": "https://upload.wikimedia.org/wikipedia/commons/a/a7/React-icon.svg"
              }
            }
          ]
        }
      ],
      "stream": true,
      "think_mode": "Off",
      "response_length": {
        "think_on": {
          "Think_Lite": [
            "Think_Deep",
            "Think_Lite",
            "Auto"
          ]
        },
        "think_off": {
          "Long_Answer": [
            "Long_Answer",
            "Short_Answer",
            "Auto"
          ]
        }
      },
      "model": "gemma-4-26b-a4b-it-qat-gguf-UD-Q4_K_XL",
      "temperature": null,
      "top_p": null,
      "top_k": null,
      "min_p": null,
      "repetition_penalty": null,
      "max_tokens": null,
      "keep_alive": null
    });

    let res = client.post(&format!("{}/v1/chat/completions", BASE_URL))
        .json(&payload)
        .send()
        .await;

    let mut response = match res {
        Ok(r) => r,
        Err(e) => {
            println!("❌ [Test] Connection error: {:?}", e);
            return Err(e.into());
        }
    };

    let status = response.status();
    println!("📡 [Test] Response Status: {}", status);
    
    if !status.is_success() {
        let err_text = response.text().await?;
        println!("❌ [Test] API Error Response: {}", err_text);
        return Err(anyhow::anyhow!("API returned error status: {}", status));
    }

    println!("📥 [Test] Streaming response tokens in real-time:");
    println!("--------------------------------------------------");
    
    let mut full_response = String::new();

    // Read chunks sequentially using reqwest
    while let Some(chunk) = response.chunk().await? {
        let text = String::from_utf8_lossy(&chunk);
        
        for line in text.lines() {
            if line.starts_with("data: ") {
                let json_str = &line["data: ".len()..];
                if json_str.trim() == "[DONE]" {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                        print!("{}", content);
                        std::io::Write::flush(&mut std::io::stdout())?;
                        full_response.push_str(content);
                    }
                }
            }
        }
    }
    
    println!("\n--------------------------------------------------");
    println!("✅ [Test] Multimodal Completion API test completed successfully!");
    
    // Save output to test/multimodal_completion_output.txt
    std::fs::create_dir_all("test")?;
    let output_path = "test/multimodal_completion_output.txt";
    std::fs::write(output_path, &full_response)?;
    println!("💾 Streamed response saved to: {}", output_path);

    Ok(())
}
