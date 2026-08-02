use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use engines::models::manager::hf_hub::HuggingFaceHub;
use engines::models::fetch::tts_resolver::TtsAssetResolver;

#[derive(Debug, Serialize, Deserialize)]
struct VariantDetails {
    variant_id: String,
    format: String,
    precision_quant_tag: String,
    architecture: String,
    total_download_size_gb: f64,
    bundled_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TerminalSelectionFlow {
    raw_input_choices: Vec<String>,
    variants: Vec<VariantDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RepoAuditItem {
    repo_id: String,
    detected_family: String,
    terminal_selection_flow: TerminalSelectionFlow,
    siblings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MasterAuditReport {
    timestamp: String,
    total_repos_audited: usize,
    successful_audits: usize,
    audit_results: Vec<RepoAuditItem>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Cluaiz Engine Downloader Audit Suite for Top Repos...");

    let json_repos_path = Path::new(r"C:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\tts_repo_audit\src\top_repos.json");
    if !json_repos_path.exists() {
        eprintln!("❌ Error: top_repos.json file not found at {:?}", json_repos_path);
        return Ok(());
    }

    let content = fs::read_to_string(json_repos_path)?;
    let target_repos: Vec<String> = serde_json::from_str(&content)?;

    println!("📋 Target Repositories Loaded: {} active models", target_repos.len());

    let mut audit_results = Vec::new();
    let mut success_count = 0;

    for repo_id in &target_repos {
        println!("🔎 Invoking Cluaiz Engine Downloader for repo [{}]...", repo_id);

        // Fetch raw items tree list directly
        let raw_tree = match HuggingFaceHub::list_raw_tree(repo_id).await {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("⚠️ Cluaiz Engine Downloader Error for repo {}: {}", repo_id, e);
                continue;
            }
        };

        let siblings: Vec<String> = raw_tree.iter().map(|item| item.path.clone()).collect();

        // Get group variants listing
        match HuggingFaceHub::list_variants(repo_id).await {
            Ok(variants) => {
                let all_files_flat: Vec<String> = variants.iter().flat_map(|v| v.all_files.clone()).collect();
                let detected_family = TtsAssetResolver::detect_tts_family(repo_id, &all_files_flat).to_string();

                let mut raw_input_choices = Vec::new();
                let mut variant_reports = Vec::new();

                for v in variants {
                    let arch_name = if detected_family.contains("kokoro") {
                        "Style_text_to_speech_2".to_string()
                    } else {
                        "ONNX Graph".to_string()
                    };

                    let format_upper = v.format_type.to_uppercase();
                    let choice_label = format!("{} {} ({}) ({:.2} GB)", format_upper, v.quant_tag, v.filename.split('/').last().unwrap_or(""), v.size_gb);
                    raw_input_choices.push(choice_label);

                    variant_reports.push(VariantDetails {
                        variant_id: v.variant_id,
                        format: format_upper,
                        precision_quant_tag: v.quant_tag,
                        architecture: arch_name,
                        total_download_size_gb: (v.size_gb * 100.0).round() / 100.0,
                        bundled_files: v.all_files,
                    });
                }

                success_count += 1;
                audit_results.push(RepoAuditItem {
                    repo_id: repo_id.clone(),
                    detected_family,
                    terminal_selection_flow: TerminalSelectionFlow {
                        raw_input_choices,
                        variants: variant_reports,
                    },
                    siblings,
                });
            }
            Err(e) => {
                eprintln!("⚠️ Cluaiz Engine Downloader list_variants Error for repo {}: {}", repo_id, e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    }

    let report = MasterAuditReport {
        timestamp: "2026-08-01T19:15:00Z".to_string(),
        total_repos_audited: target_repos.len(),
        successful_audits: success_count,
        audit_results,
    };

    let report_json = serde_json::to_string_pretty(&report)?;
    let output_path = Path::new(r"C:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\tts_repo_audit\tts_models_audit_report.json");
    fs::write(output_path, &report_json)?;

    println!("============================================================");
    println!("✅ Cluaiz Downloader Audit Complete! Audited: {}/{}", success_count, target_repos.len());
    println!("📄 Audit JSON Saved: {:?}", output_path);
    println!("============================================================");

    Ok(())
}
