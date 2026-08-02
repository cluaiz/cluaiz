use serde_json::Value;
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Starting HuggingFace Repository Structure Prober...");

    let top_repos_path = Path::new(r"C:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\tts_repo_audit\src\top_repos.json");
    if !top_repos_path.exists() {
        eprintln!("❌ Error: top_repos.json not found.");
        return Ok(());
    }

    let top_repos_content = fs::read_to_string(top_repos_path)?;
    let repo_ids: Vec<String> = serde_json::from_str(&top_repos_content)?;

    let output_dir = Path::new(r"C:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\tts_repo_audit\src\raw_trees");
    fs::create_dir_all(output_dir)?;

    let client = reqwest::Client::new();

    for repo_id in repo_ids {
        println!("🚀 Fetching tree for: {}", repo_id);
        let safe_name = repo_id.replace('/', "__");
        let output_file_path = output_dir.join(format!("{}.json", safe_name));

        if output_file_path.exists() {
            println!("  ✅ Cache hit, skipping.");
            continue;
        }

        let api_url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=true", repo_id);
        
        let response = client.get(&api_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json_val) = resp.json::<Value>().await {
                        let pretty_json = serde_json::to_string_pretty(&json_val)?;
                        fs::write(&output_file_path, pretty_json)?;
                        println!("  💾 Saved raw tree to {:?}", output_file_path);
                    } else {
                        eprintln!("  ❌ Failed to parse JSON response.");
                    }
                } else {
                    eprintln!("  ❌ API response error: {}", resp.status());
                }
            }
            Err(e) => {
                eprintln!("  ❌ Network error: {}", e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    println!("🎉 All top 100 raw trees synced inside 'raw_trees' directory!");
    Ok(())
}
