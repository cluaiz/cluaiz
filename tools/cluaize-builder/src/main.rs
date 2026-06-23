use cluaize_shared::environment::EnvironmentManager;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::fs;

fn main() {
    println!("🚀 Starting Unified Cluaize Build System...");

    // Default settings
    let mut mode = "dev".to_string();
    let mut profile = "release".to_string();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" | "-m" => {
                if i + 1 < args.len() {
                    mode = args[i + 1].clone();
                    i += 1;
                }
            }
            "--profile" | "-p" => {
                if i + 1 < args.len() {
                    profile = args[i + 1].clone();
                    i += 1;
                }
            }
            "--help" | "-h" => {
                println!("Usage: cargo run -- [OPTIONS]");
                println!("Options:");
                println!("  --mode, -m <dev|public>      Deployment mode (default: dev)");
                println!("  --profile, -p <debug|release> Build profile (default: release)");
                return;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if mode != "dev" && mode != "public" {
        eprintln!("❌ Invalid mode: {}. Use 'dev' or 'public'.", mode);
        std::process::exit(1);
    }
    if profile != "debug" && profile != "release" {
        eprintln!("❌ Invalid profile: {}. Use 'debug' or 'release'.", profile);
        std::process::exit(1);
    }

    println!("📋 Settings Loaded: Mode = [{}], Profile = [{}]", mode.to_uppercase(), profile.to_uppercase());

    let mut cargo_args = vec!["build", "--workspace"];
    if profile == "release" {
        cargo_args.insert(1, "--release");
    }

    // 1. Build entire workspace: cluaize.exe + cluaize_llama.dll + cluaize_onnx.dll
    // NOTE: This is a unified workspace. All artifacts land in a single target/release/.
    // There is NO separate target/ per crate. Do NOT add separate driver build steps.
    println!("⚙️  Building entire Cluaize workspace (cluaize.exe + drivers)...");
    let status = Command::new("cargo")
        .args(&cargo_args)
        .status()
        .expect("Failed to execute cargo build --workspace");
    
    if !status.success() {
        eprintln!("❌ Workspace build failed!");
        std::process::exit(1);
    }

    // 4. Resolve Environments
    println!("🔍 Resolving Environment Paths...");
    
    // Override EnvironmentManager logic based on 'mode' argument
    if mode == "public" {
        // Force the environment to Global (Installed) mode
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        std::env::set_var("CLUAIZE_HOME", home_dir.join(".cluaize").to_str().unwrap());
        // Remove CARGO var so EnvironmentManager doesn't get tricked into Dev mode
        std::env::remove_var("CARGO"); 
    } else {
        // Dev Mode: Force local path
        let current_dir = std::env::current_dir().unwrap();
        std::env::set_var("CLUAIZE_HOME", current_dir.join(".cluaize").to_str().unwrap());
    }

    let env = EnvironmentManager::current();
    println!("   Active Deployment Mode: {:?}", env.mode);
    println!("   Target Root Directory: {:?}", env.root_dir);

    let kernel_dir = env.ensure_kernel_dir().expect("Failed to ensure kernel dir");
    let drivers_dir = env.ensure_drivers_dir().expect("Failed to ensure drivers dir");
    let bin_dir = env.root_dir.join("bin");
    
    if !bin_dir.exists() {
        fs::create_dir_all(&bin_dir).expect("Failed to create bin dir");
    }

    // 5. Deploy Binaries
    println!("🚚 Deploying Binaries...");
    
    let copy_file = |src: &Path, dest_dir: &Path| {
        if !src.exists() {
            eprintln!("   ⚠️ Missing artifact: {:?}", src);
            return;
        }
        let file_name = src.file_name().unwrap();
        let dest = dest_dir.join(file_name);
        fs::copy(src, &dest).unwrap_or_else(|e| {
            panic!("Failed to copy {:?} to {:?}: {}", src, dest, e);
        });
        println!("   ✅ Copied {:?} -> {:?}", src.file_name().unwrap(), dest);
    };

    let exe_ext = if cfg!(windows) { ".exe" } else { "" };
    let dll_ext = if cfg!(windows) { ".dll" } else if cfg!(target_os = "macos") { ".dylib" } else { ".so" };
    let dll_prefix = if cfg!(windows) { "" } else { "lib" };

    let current_dir = std::env::current_dir().unwrap();
    // All artifacts from --workspace land in ONE unified target dir. No per-crate dirs.
    let unified_target_dir = current_dir.join("target").join(&profile);

    // ── Deployment Layout (traced from actual dependency chain) ──
    //
    // DEPENDENCY CHAIN (verified via Cargo.toml):
    //   cmd/cluaize.exe → cluaize_api (rlib) → dispatcher (rlib) → llama (rlib)
    //   ∴ dispatcher code is STATICALLY LINKED into cluaize.exe. No separate .dll needed.
    //   ∴ dispatcher.dll in target/release/ is a side-effect of crate-type=["cdylib","rlib"].
    //   ∴ No GitHub workflow exists for dispatcher. It is NOT a deployed artifact.
    //
    // What IS deployed as separate dynamic libraries:
    //   cluaize_llama.dll — loaded by dispatcher code at runtime via libloading FFI
    //   cluaize_onnx.dll  — loaded by EmbeddingDispatcher at runtime via libloading FFI
    //   engines.dll       — core inference orchestrator (deployed via cluaize-engine.yml CI)
    //
    // dispatcher.rs expects files in: HardwareGovernor::resolve_interface_path()/kernels/
    //   → engine/interfaces/kernels/
    //   → filenames: "cluaize-llama.dll" (dash) + ".ready" marker

    let cluaize_exe   = unified_target_dir.join(format!("cluaize{}", exe_ext));
    let llama_dll_src = unified_target_dir.join(format!("{}cluaize_llama{}", dll_prefix, dll_ext));
    let onnx_dll_src  = unified_target_dir.join(format!("{}cluaize_onnx{}", dll_prefix, dll_ext));
    let engines_dll   = unified_target_dir.join(format!("{}engines{}", dll_prefix, dll_ext));

    // Deploy: CLI binary (contains dispatcher statically) → bin/
    copy_file(&cluaize_exe, &bin_dir);

    // Deploy: Core engine orchestrator → engine/interfaces/kernels/
    copy_file(&engines_dll, &kernel_dir);

    // Deploy: LLaMA driver → kernels/ with BOTH names (underscore + dash) + .ready marker
    // dispatcher.rs line 140 looks for "cluaize-llama.dll" (dash), build produces "cluaize_llama.dll"
    if llama_dll_src.exists() {
        let dash_name = format!("{}cluaize-llama{}", dll_prefix, dll_ext);
        let dest_dash = kernel_dir.join(&dash_name);
        let dest_us   = kernel_dir.join(format!("{}cluaize_llama{}", dll_prefix, dll_ext));
        fs::copy(&llama_dll_src, &dest_dash).expect("Failed to copy cluaize-llama.dll");
        fs::copy(&llama_dll_src, &dest_us).expect("Failed to copy cluaize_llama.dll");
        // Write .ready marker that dispatcher strictly validates (lib.rs line 147-156)
        fs::write(kernel_dir.join("cluaize-llama.ready"), b"ready").expect("Failed to write llama.ready");
        println!("   \u{2705} Deployed cluaize-llama.dll + cluaize_llama.dll + .ready \u2192 {:?}", kernel_dir);
    } else {
        eprintln!("   \u{26a0}\u{fe0f} Missing artifact: {:?}", llama_dll_src);
    }

    // Deploy: ONNX driver → kernels/ with BOTH names + .ready marker
    // dispatcher.rs line 292 looks for "cluaize-onnx.dll" (dash), build produces "cluaize_onnx.dll"
    if onnx_dll_src.exists() {
        let dash_name = format!("{}cluaize-onnx{}", dll_prefix, dll_ext);
        let dest_dash = kernel_dir.join(&dash_name);
        let dest_us   = kernel_dir.join(format!("{}cluaize_onnx{}", dll_prefix, dll_ext));
        fs::copy(&onnx_dll_src, &dest_dash).expect("Failed to copy cluaize-onnx.dll");
        fs::copy(&onnx_dll_src, &dest_us).expect("Failed to copy cluaize_onnx.dll");
        // Write .ready marker that dispatcher strictly validates (lib.rs line 300-306)
        fs::write(kernel_dir.join("cluaize-onnx.ready"), b"ready").expect("Failed to write onnx.ready");
        println!("   \u{2705} Deployed cluaize-onnx.dll + cluaize_onnx.dll + .ready \u2192 {:?}", kernel_dir);
    } else {
        eprintln!("   \u{26a0}\u{fe0f} Missing artifact: {:?}", onnx_dll_src);
    }

    println!("\u{1f389} Cluaize Environment Successfully Deployed!");
}
