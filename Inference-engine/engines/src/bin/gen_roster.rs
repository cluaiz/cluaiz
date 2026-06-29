use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub architecture: String,
    pub parameters: String,
    pub training_tokens: String,
    pub bit_depth: String,
    pub ram_required_gb: f64,
    pub tps_estimate: u32,
    pub huggingface_repo: String,
    pub description: String,
    pub is_cloud_api: bool,
}

#[derive(Debug, Serialize)]
pub struct RosterFile {
    pub models: Vec<ModelManifest>,
}

fn generate_repo(name: &str) -> String {
    let lower_name = name.to_lowercase();
    if lower_name.contains("bonsai") || lower_name.contains("cluaiz") {
        let first = name.split_whitespace().next().unwrap().replace("(", "").replace(")", "");
        format!("cluaiz/{}", first.to_lowercase())
    } else if lower_name.contains("qwen") {
        format!("Qwen/{}-GGUF", name.split_whitespace().next().unwrap())
    } else if lower_name.contains("llama") {
        format!("meta-llama/{}-Instruct", name.split_whitespace().next().unwrap().replace("-Instruct", ""))
    } else if lower_name.contains("gemma") {
        format!("google/{}-it", name.split_whitespace().next().unwrap().to_lowercase())
    } else if lower_name.contains("mistral") || lower_name.contains("ministral") {
        format!("mistralai/{}", name.split_whitespace().next().unwrap())
    } else if lower_name.contains("deepseek") {
        format!("deepseek-ai/{}", name.split_whitespace().next().unwrap())
    } else if lower_name.contains("phi") {
        format!("microsoft/{}", name.split_whitespace().next().unwrap())
    } else if lower_name.contains("bitnet") {
        "1bitLLM/bitnet_b1_58-large".to_string()
    } else if lower_name.contains("smollm") {
        format!("HuggingFaceTB/{}", name.split_whitespace().next().unwrap())
    } else if lower_name.contains("granite") {
        format!("ibm/{}-instruct", name.split_whitespace().next().unwrap().to_lowercase())
    } else if lower_name.contains("starcoder") {
        format!("bigcode/{}", name.split_whitespace().next().unwrap().to_lowercase())
    } else {
        format!("local/{}-gguf", name.split_whitespace().next().unwrap().to_lowercase())
    }
}

fn generate_detailed_description(name: &str, params: &str, strength: &str, arch: &str, bit_depth: &str, tokens: &str) -> String {
    format!(
        "The {} model utilizes a {} parameter architecture tailored for '{}'. It operates at the {} layer and runs locally on user hardware via the cluaiz Inference Engine. Pre-trained on a {} corpus, it supports offline execution for logic, mathematics, and coding syntax. Powered by the {} architecture, it ensures complete data privacy for cross-platform deployment.",
        name, params, strength, bit_depth, tokens, arch
    )
}

fn main() {
    let txt_path = Path::new(r"c:\Users\Aryan\my\cluaiz-workspace\bitnetmocle.txt");
    let content = fs::read_to_string(txt_path).expect("Failed to read bitnetmocle.txt");

    let mut models = Vec::new();
    let mut past_header = false;

    for line in content.lines() {
        if line.contains("Model Name") {
            past_header = true;
            continue;
        }
        if !past_header || line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let name = parts[0].trim();
            let params = parts[1].trim();
            let tokens = parts[2].trim();
            let ram_str = parts[3].replace("GB", "").trim().to_string();
            let ram = ram_str.parse::<f64>().unwrap_or(0.0);
            let bit_depth = parts[4].trim();
            let tps_str = parts[5].replace("+", "").replace("-", "0").trim().to_string();
            
            // Just grab numbers
            let tps: u32 = tps_str.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            let strength = parts[6].trim();

            let is_cloud = name.to_lowercase().contains("grok") || bit_depth.to_lowercase().contains("api");
            let repo = if is_cloud { "api:openai/grok".to_string() } else { generate_repo(name) };

            let arch = if name.contains('-') { name.split('-').next().unwrap() } else { name.split_whitespace().next().unwrap() };
            
            let description = if is_cloud {
                format!("This is a powerful Cloud API bridging ({}) architecture. Serving massive scale without local hardware requirements. Capable of '{}' with near-instant reasoning throughput due to remote data centers. It heavily utilizes API routing protocols guaranteeing extreme context length management and secure parameter bounds handling.", name, strength)
            } else {
                generate_detailed_description(name, params, strength, arch, bit_depth, tokens)
            };

            models.push(ModelManifest {
                id: name.to_lowercase().replace(" ", "-").replace("(", "").replace(")", ""),
                name: name.to_string(),
                architecture: arch.to_string(),
                parameters: params.to_string(),
                training_tokens: tokens.to_string(),
                bit_depth: bit_depth.to_string(),
                ram_required_gb: ram,
                tps_estimate: tps,
                huggingface_repo: repo,
                description,
                is_cloud_api: is_cloud,
            });
        }
    }

    let out = RosterFile { models };
    let json_str = serde_json::to_string_pretty(&out).unwrap();

    let out_path = Path::new(r"c:\Users\Aryan\my\cluaiz-workspace\cluaiz-OS\cluaiz-ai-\engines\src\default_roster.json");
    fs::write(out_path, json_str).expect("Failed to write fast json");
    
    println!("✅ Perfectly generated default_roster.json using 100% Rust!");
}
