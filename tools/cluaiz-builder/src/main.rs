mod local_ci;
mod media_assets;

use std::env;
use std::process::Command;

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
    println!("  media-assets      Stage a verified native package: --source <package> --output <new-directory>");
    println!("");
    println!("Options:");
    println!("  --profile <mode>  Build profile: 'debug' (default) or 'release'");
    println!("  --help, -h        Print this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|command| command == "media-assets") {
        if let Err(error) = media_assets::run(&args[2..]) {
            eprintln!("Native media packaging failed: {error:#}");
            std::process::exit(1);
        }
        return;
    }

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
                    eprintln!(
                        "❌ Error: 'driver' command requires a driver name (e.g., llama or onnx)"
                    );
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
        eprintln!(
            "❌ Error: Invalid profile '{}'. Use 'debug' or 'release'.",
            profile
        );
        std::process::exit(1);
    }

    println!(
        "📋 Target: [{}] | Profile: [{}]",
        command_type.to_uppercase(),
        profile.to_uppercase()
    );

    let mut commands_to_run = Vec::new();

    match command_type.as_str() {
        "all" => {
            println!("⚙️  Building entire cluaiz workspace...");
            let mut ws_cmd = vec!["build", "--workspace"];
            if profile == "release" {
                ws_cmd.push("--release");
            }
            commands_to_run.push(("Workspace", ws_cmd));
        }
        "core" => {
            println!("⚙️  Building Core Unified Server & CLI Binary...");
            let mut cmd = vec!["build", "-p", "cmd"];
            if profile == "release" {
                cmd.push("--release");
            }
            commands_to_run.push(("Core Binary (cmd)", cmd));
        }
        "drivers" => {
            println!("⚙️  Building Engine Crates...");
            let mut llama_cmd = vec!["build", "-p", "llama-engine"];
            if profile == "release" {
                llama_cmd.push("--release");
            }
            commands_to_run.push(("Engine: LLaMA", llama_cmd));

            let mut onnx_cmd = vec!["build", "-p", "cluaiz-onnx"];
            if profile == "release" {
                onnx_cmd.push("--release");
            }
            commands_to_run.push(("Engine: ONNX", onnx_cmd));
        }
        "driver" => {
            let crate_name = match driver_name.as_str() {
                "llama" => "llama-engine",
                "onnx" => "cluaiz-onnx",
                other => other,
            };
            println!("⚙️  Building Specific Crate: {} ...", crate_name);
            let mut cmd = vec!["build", "-p", crate_name];
            if profile == "release" {
                cmd.push("--release");
            }
            commands_to_run.push(("Engine Crate", cmd));
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

    println!(
        "✅ Build Successful! Binary located at target/{}/cluaiz.exe",
        profile
    );
}
