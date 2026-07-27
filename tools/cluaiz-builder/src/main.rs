mod local_ci;

use std::process::Command;
use std::env;

fn print_help() {
    println!("🚀 cluaiz Modular Builder");
    println!("Usage: cargo run -p cluaiz-builder -- <COMMAND> [OPTIONS]");
    println!("");
    println!("Commands:");
    println!("  all               Build the entire workspace (Core + All Drivers + CLI)");
    println!("  core              Build only the Core Engine and CLI (cluaiz, engines)");
    println!("  drivers           Build all hardware drivers (llama, onnx)");
    println!("  onnx              Build the ONNX hardware driver in isolation");
    println!("  llama             Build the LLaMA hardware driver in isolation");
    println!("  driver <name>     Build a specific driver (e.g., 'llama' or 'onnx')");
    println!("");
    println!("Options:");
    println!("  --profile <mode>  Build profile: 'debug' (default) or 'release'");
    println!("  --help, -h        Print this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        print_help();
        std::process::exit(1);
    }

    let mut command_type = String::new();
    let mut driver_name = String::new();
    let mut profile = "debug".to_string(); // Default to debug

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "all" | "core" | "drivers" => {
                command_type = args[i].clone();
            }
            "onnx" | "llama" => {
                command_type = "driver".to_string();
                driver_name = args[i].clone();
            }
            "driver" => {
                command_type = "driver".to_string();
                if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    driver_name = args[i + 1].clone();
                    i += 1;
                } else {
                    eprintln!("❌ Error: 'driver' command requires a driver name (e.g., llama or onnx)");
                    std::process::exit(1);
                }
            }
            "--profile" | "-p" => {
                if i + 1 < args.len() {
                    profile = args[i + 1].clone();
                    i += 1;
                }
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {
                // Ignore unknown args for now or skip them
            }
        }
        i += 1;
    }

    if command_type.is_empty() {
        eprintln!("❌ Error: No valid command provided.");
        print_help();
        std::process::exit(1);
    }

    if profile != "debug" && profile != "release" {
        eprintln!("❌ Error: Invalid profile '{}'. Use 'debug' or 'release'.", profile);
        std::process::exit(1);
    }

    println!("📋 Target: [{}] | Profile: [{}]", command_type.to_uppercase(), profile.to_uppercase());

    let mut commands_to_run = Vec::new();

    match command_type.as_str() {
        "all" => {
            println!("⚙️  Building entire cluaiz workspace...");
            let mut ws_cmd = vec!["build", "--workspace"];
            if profile == "release" { ws_cmd.push("--release"); }
            commands_to_run.push(("Workspace", ws_cmd));

            let mut llama_cmd = vec!["build", "--manifest-path", "interface-engines/llama/Cargo.toml"];
            if profile == "release" { llama_cmd.push("--release"); }
            commands_to_run.push(("Driver: Llama", llama_cmd));

            let mut onnx_cmd = vec!["build", "--manifest-path", "interface-engines/onnx/Cargo.toml"];
            if profile == "release" { onnx_cmd.push("--release"); }
            commands_to_run.push(("Driver: ONNX", onnx_cmd));
        }
        "core" => {
            println!("⚙️  Building Core Engine & CLI...");
            let mut cmd = vec!["build", "-p", "cmd", "-p", "engines"];
            if profile == "release" { cmd.push("--release"); }
            commands_to_run.push(("Core", cmd));
        }
        "drivers" => {
            println!("⚙️  Building All Drivers...");
            let mut llama_cmd = vec!["build", "--manifest-path", "interface-engines/llama/Cargo.toml"];
            if profile == "release" { llama_cmd.push("--release"); }
            commands_to_run.push(("Driver: Llama", llama_cmd));

            let mut onnx_cmd = vec!["build", "--manifest-path", "interface-engines/onnx/Cargo.toml"];
            if profile == "release" { onnx_cmd.push("--release"); }
            commands_to_run.push(("Driver: ONNX", onnx_cmd));
        }
        "driver" => {
            println!("⚙️  Building Specific Driver: {} ...", driver_name);
            let manifest_path = format!("interface-engines/{}/Cargo.toml", driver_name);
            // Verify path exists to avoid confusing errors
            if !std::path::Path::new(&manifest_path).exists() {
                eprintln!("❌ Error: Driver manifest not found at {}", manifest_path);
                std::process::exit(1);
            }
            let manifest_path_static = Box::leak(manifest_path.into_boxed_str());
            let mut cmd = vec!["build", "--manifest-path", manifest_path_static];
            if profile == "release" { cmd.push("--release"); }
            commands_to_run.push(("Driver", cmd));
        }
        _ => unreachable!(),
    }

    for (name, mut args) in commands_to_run {
        // --- LOCAL FIRST CI ARCHITECTURE ---
        let target_driver = if name == "Driver" {
            driver_name.to_lowercase()
        } else if name.starts_with("Driver: ") {
            name.strip_prefix("Driver: ").unwrap().to_lowercase()
        } else {
            String::new()
        };

        if !target_driver.is_empty() {
            let extra_args = local_ci::execute_local_ci_for_driver(&target_driver, &profile);
            for arg in extra_args {
                args.push(Box::leak(arg.into_boxed_str()));
            }
        }
        // -----------------------------------

        println!("🚀 Executing [{}] -> cargo {}", name, args.join(" "));

        let status = Command::new("cargo")
            .args(&args)
            .status()
            .expect("Failed to execute cargo build");

        if !status.success() {
            eprintln!("❌ Build failed for target: {}", name);
            std::process::exit(1);
        }
    }

    // Auto-sync compiled driver DLLs directly to .cluaiz runtime folders
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let engine_dir = root.join(".cluaiz").join("engine");
    let drivers_dir = engine_dir.join("drivers");
    let target_dir = if profile == "release" { root.join("target").join("release") } else { root.join("target").join("debug") };

    let ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };

    if engine_dir.exists() {
        let drivers_to_sync = if !driver_name.is_empty() {
            vec![driver_name.clone()]
        } else {
            vec!["onnx".to_string(), "llama".to_string()]
        };

        for d in drivers_to_sync {
            let src_name = format!("cluaiz_{}.{}", d, ext);
            let alt_src_name = format!("{}.{}", d, ext);
            let src_path = if target_dir.join(&src_name).exists() {
                target_dir.join(&src_name)
            } else if target_dir.join(&alt_src_name).exists() {
                target_dir.join(&alt_src_name)
            } else {
                continue;
            };

            let dest_name = format!("cluaiz-{}.{}", d, ext);
            let dest_path = engine_dir.join(&dest_name);
            
            if std::fs::copy(&src_path, &dest_path).is_ok() {
                println!("🧬 [cluaiz-builder] Auto-synced engine: {:?}", dest_path);
            }

            if drivers_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&drivers_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                            if fname.contains(&d) || (d == "onnx" && fname.contains("cuda")) {
                                if std::fs::copy(&src_path, &p).is_ok() {
                                    println!("🧬 [cluaiz-builder] Overwrote active driver DLL: {:?}", p);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("✅ Build & Sync Successful!");
}
