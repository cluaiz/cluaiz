use std::fs::File;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let path = Path::new(r"C:\Users\Aryan\my\cluaiz-workspace\cluaiz-OS\cluaiz-ai-\terminal_ui\models\models--Qwen--Qwen3-4B-GGUF\Qwen3-4B-Q4_K_M.gguf");
    if !path.exists() {
        println!("❌ File not found!");
        return Ok(());
    }

    let mut file = File::open(path)?;
    let content = candle_core::quantized::gguf_file::Content::read(&mut file)?;

    println!("🔍 Scanning GGUF Metadata for Qwen3...");
    for (key, _) in content.metadata.iter() {
        if key.contains("qwen") || key.contains("attention") || key.contains("block") {
            println!("Key: {}", key);
        }
    }

    Ok(())
}
