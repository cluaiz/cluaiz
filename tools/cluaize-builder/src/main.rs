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
            "driver" => {
                command_type = "driver".to_string();
                if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    driver_name = args[i + 1].clone();
                    i += 1;
                } else {
                    eprintln!("❌ Error: 'driver' command requires a driver name (e.g., llama)");
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

    for (name, args) in commands_to_run {
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

    // Since Bootstrapper inside cluaiz.exe handles all the 1:1 artifact syncing
    // (copying to ~/.cluaiz/engine/ and renaming to dashed-names),
    // we do NOT manually copy files here.
    // JSON configs (Permission.json, system_control.json) are auto-generated
    // by the engine natively upon first startup.
    
    println!("✅ Build Successful!");
    println!("💡 Note: The cluaiz Bootstrapper will automatically sync these artifacts to your ~/.cluaiz directory the next time you run 'cluaiz'.");
}
