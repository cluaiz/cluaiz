use std::path::Path;
use std::fs::File;
use std::io::Write;
use ort::session::Session;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut log_file = File::create("c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/test/model_header_diagnostic_report.txt")?;
    
    macro_rules! log_and_print {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            println!("{}", line);
            writeln!(log_file, "{}", line)?;
        }};
    }

    log_and_print!("==================================================");
    log_and_print!("🚀 [CLUAIZ MODEL HEADER DIAGNOSTIC SUITE] 🚀");
    log_and_print!("==================================================");

    let models_dir = Path::new("C:/Users/Aryan/.cluaiz/models/audio");
    if !models_dir.exists() {
        log_and_print!("❌ Model directory not found: {:?}", models_dir);
        return Ok(());
    }

    let entries = std::fs::read_dir(models_dir)?;
    for entry in entries.flatten() {
        let model_path = entry.path();
        if !model_path.is_dir() {
            continue;
        }

        let model_name = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown");
        log_and_print!("\n📦 [MODEL PACKAGE INSPECTION]: {}", model_name);
        log_and_print!("   Path: {:?}", model_path);

        let sub_entries = std::fs::read_dir(&model_path)?;
        for sub in sub_entries.flatten() {
            let sub_path = sub.path();
            if sub_path.extension().and_then(|e| e.to_str()) == Some("onnx") {
                let file_name = sub_path.file_name().unwrap().to_string_lossy();
                log_and_print!("\n   🔍 Probing ONNX Graph Header: {}", file_name);

                let builder = Session::builder()?;
                match builder.commit_from_file(&sub_path) {
                    Ok(session) => {
                        log_and_print!("      ✅ Session Commit: SUCCESS (Loaded into ORT Engine)");
                        log_and_print!("      📥 Graph Inputs (Header Signature):");
                        for input in session.inputs() {
                            log_and_print!("         - Input Name: {}", input.name());
                        }
                        log_and_print!("      📤 Graph Outputs (Header Signature):");
                        for output in session.outputs() {
                            log_and_print!("         - Output Name: {}", output.name());
                        }
                    }
                    Err(e) => {
                        log_and_print!("      ❌ Session Commit: FAILED -> Error: {}", e);
                    }
                }
            }
        }
    }

    log_and_print!("\n==================================================");
    log_and_print!("✅ [DIAGNOSTIC COMPLETE] Report saved to model_header_diagnostic_report.txt");
    log_and_print!("==================================================");

    Ok(())
}
