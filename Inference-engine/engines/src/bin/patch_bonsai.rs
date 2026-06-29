use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   cluaiz Core OS: cluaiz BONSAI BINARY PATCHER v1.0       ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("⚠️ Usage: patch_bonsai <path_to_gguf_file>");
        return Ok(());
    }

    let input_path = PathBuf::from(&args[1]);
    if !input_path.exists() {
        println!("❌ Target file not found: {}", input_path.display());
        return Ok(());
    }

    // Determine output path (create a copy)
    let parent_dir = input_path.parent().unwrap_or(Path::new(""));
    let mut file_name = input_path.file_stem().unwrap().to_os_string();
    file_name.push("-Patched.gguf");
    let output_path = parent_dir.join(file_name);

    println!("📥 Loading Target File into RAM (this may take a moment)...");
    println!("   Source: {}", input_path.display());
    
    let start_time = Instant::now();
    let mut data = fs::read(&input_path)?;
    println!("   Loaded {} bytes in {:.2?}", data.len(), start_time.elapsed());

    println!("\n🔍 Scanning file for 'qwen3' architecture tags...");
    let target = b"qwen3";
    let replacement = b"qwen2";
    let mut patch_count = 0;

    let mut i = 0;
    while i <= data.len() - target.len() {
        if &data[i..i + target.len()] == target {
            // Found a match! Patch the '3' to '2' in memory
            for j in 0..target.len() {
                data[i + j] = replacement[j];
            }
            println!("   ✅ Patched at memory offset: {:#010X}", i);
            patch_count += 1;
            // Skip the replaced bytes
            i += target.len();
        } else {
            i += 1;
        }
    }

    if patch_count > 0 {
        println!("\n💾 Flushing patched binary to disk...");
        println!("   Destination: {}", output_path.display());
        let write_start = Instant::now();
        fs::write(&output_path, &data)?;
        println!("   Written {} bytes in {:.2?}", data.len(), write_start.elapsed());
        
        println!("\n🎉 SUCCESS! Patched {} occurrences seamlessly.", patch_count);
        println!("   The 1-bit native model is now ready for cluaiz Ignition.");
    } else {
        println!("\n⚠️ No occurrences of 'qwen3' found. File might already be patched or irrelevant.");
    }

    Ok(())
}
