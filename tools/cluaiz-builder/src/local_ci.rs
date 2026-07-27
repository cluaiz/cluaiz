use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde_yaml::Value;
use cluaiz_shared::HardwareGovernor;

pub fn execute_local_ci_for_driver(driver: &str, profile: &str) -> Vec<String> {
    println!("🔍 [Local-CI] Auditing system_control.json & GitHub Action Workflows for [{}] driver...", driver);
    
    let control = match HardwareGovernor::load_system_control() {
        Ok(c) => c,
        Err(_) => {
            println!("⚠️ [Local-CI] Failed to load system_control.json. Skipping local CI workflow step.");
            return Vec::new();
        }
    };

    // 1. Locate workflow YAML file dynamically: .github/workflows/cluaize-<driver>-driver.yml
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let root_dir = if manifest_dir.join("Cargo.toml").exists() && manifest_dir.parent().map_or(false, |p| p.join("Cargo.toml").exists()) {
        manifest_dir.parent().unwrap().to_path_buf()
    } else {
        manifest_dir.parent().and_then(|p| p.parent()).unwrap_or(&manifest_dir).to_path_buf()
    };

    let yml_name = format!("cluaize-{}-driver.yml", driver);
    let yml_path = root_dir.join(".github/workflows").join(&yml_name);

    if !yml_path.exists() {
        println!("ℹ️ [Local-CI] No workflow matrix found at {:?}. Defaulting to standard compilation.", yml_path);
        return Vec::new();
    }

    println!("📄 [Local-CI] Parsing workflow matrix from {:?}...", yml_name);
    let yml_str = match fs::read_to_string(&yml_path) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️ [Local-CI] Failed to read workflow file: {}", e);
            return Vec::new();
        }
    };

    let yaml: Value = match serde_yaml::from_str(&yml_str) {
        Ok(y) => y,
        Err(e) => {
            println!("⚠️ [Local-CI] Failed to parse YAML: {}", e);
            return Vec::new();
        }
    };

    // 2. Detect Host OS & Hardware Profile
    let host_os = if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") { "mac-arm64" } else { "mac-x64" }
    } else {
        "linux-x64"
    };

    let mut detected_backend = "cpu";
    for gpu in &control.silicon_truth.accelerators.gpus {
        let vendor = gpu.vendor.to_uppercase();
        if vendor.contains("NVIDIA") {
            detected_backend = "cuda";
            break;
        } else if vendor.contains("AMD") || vendor.contains("RADEON") || vendor.contains("ADVANCED MICRO") {
            detected_backend = "vulkan";
            break;
        } else if vendor.contains("INTEL") {
            detected_backend = "openvino";
            break;
        }
    }

    if detected_backend == "cpu" && cfg!(target_os = "macos") {
        detected_backend = "metal";
    }

    println!("🟩 [Local-CI] Hardware Audit Match -> Platform: [{}], Backend: [{}]", host_os, detected_backend);

    // 3. Match Matrix Entry in Workflow YAML
    let matrix_includes = yaml.get("jobs")
        .and_then(|j| j.get("build-drivers"))
        .and_then(|b| b.get("strategy"))
        .and_then(|s| s.get("matrix"))
        .and_then(|m| m.get("include"))
        .and_then(|i| i.as_sequence());

    let mut matched_features = String::new();

    if let Some(includes) = matrix_includes {
        for item in includes {
            let item_platform = item.get("platform").and_then(|p| p.as_str()).unwrap_or("");
            let item_backend = item.get("backend").and_then(|b| b.as_str()).unwrap_or("");
            
            if item_platform == host_os && item_backend == detected_backend {
                if let Some(feat) = item.get("features").and_then(|f| f.as_str()) {
                    matched_features = feat.to_string();
                    println!("🎯 [Local-CI] Matched Workflow Matrix Row -> Features: [{}]", matched_features);
                    break;
                }
            }
        }
    }

    // 4. Parse & Execute ONLY Steps Matching Host Backend & OS
    let steps = yaml.get("jobs")
        .and_then(|j| j.get("build-drivers"))
        .and_then(|b| b.get("steps"))
        .and_then(|s| s.as_sequence());

    if let Some(steps) = steps {
        for step in steps {
            let name = step.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let step_if = step.get("if").and_then(|i| i.as_str()).unwrap_or("");
            let run_script = step.get("run").and_then(|r| r.as_str()).unwrap_or("");

            let name_lower = name.to_lowercase();
            let step_if_lower = step_if.to_lowercase();

            // OS Filtering
            if cfg!(windows) && (name_lower.contains("(linux)") || step_if_lower.contains("linux")) {
                continue;
            }
            if !cfg!(windows) && (name_lower.contains("(windows)") || step_if_lower.contains("windows")) {
                continue;
            }

            // Backend Filtering
            if detected_backend == "cuda" && (name_lower.contains("vulkan") || name_lower.contains("openvino") || name_lower.contains("rocm") || name_lower.contains("sycl")) {
                continue;
            }
            if detected_backend == "vulkan" && (name_lower.contains("cuda") || name_lower.contains("openvino") || name_lower.contains("rocm")) {
                continue;
            }

            // Download & Setup Step Check
            if (name_lower.contains("fetch") || name_lower.contains("setup")) && (run_script.contains("http://") || run_script.contains("https://")) {
                println!("🚀 [Local-CI] Executing Matched Workflow Step: {}", name);
                
                let mut download_url = String::new();
                let mut archive_name = String::new();

                for token in run_script.split_whitespace() {
                    let clean = token.trim_matches(|c| c == '\'' || c == '"');
                    if (clean.starts_with("http://") || clean.starts_with("https://"))
                        && (clean.ends_with(".zip") || clean.ends_with(".tgz") || clean.ends_with(".tar.gz")) {
                        download_url = clean.to_string();
                        if let Some(n) = download_url.split('/').last() {
                            archive_name = n.to_string();
                        }
                        break;
                    }
                }

                if !download_url.is_empty() && !archive_name.is_empty() {
                    let target_dir = root_dir.join("target").join(profile);
                    fs::create_dir_all(&target_dir).unwrap();
                    let archive_path = target_dir.join(&archive_name);
                    let extract_dir = target_dir.join(format!("{}_custom", driver));

                    if !archive_path.exists() {
                        println!("   -> Downloading [{}] from YAML step...", archive_name);
                        let _ = Command::new("curl")
                            .args(&["-sL", &download_url, "-o", archive_path.to_str().unwrap()])
                            .status();
                    }

                    if !extract_dir.exists() && archive_path.exists() {
                        println!("   -> Extracting {} ...", archive_name);
                        let _ = Command::new("tar")
                            .args(&["-xf", archive_path.to_str().unwrap(), "-C", target_dir.to_str().unwrap()])
                            .status();
                        
                        let base_folder_name = archive_name.trim_end_matches(".zip").trim_end_matches(".tgz").trim_end_matches(".tar.gz");
                        let extracted_original = target_dir.join(base_folder_name);
                        if extracted_original.exists() {
                            let _ = fs::rename(extracted_original, &extract_dir);
                        }
                    }

                    let lib_dir = extract_dir.join("lib");
                    if lib_dir.exists() {
                        std::env::set_var("ORT_STRATEGY", "system");
                        std::env::set_var("ORT_LIB_LOCATION", lib_dir.to_str().unwrap());

                        if let Ok(entries) = fs::read_dir(&lib_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_file() && path.extension().map_or(false, |ext| ext == "dll" || ext == "so" || ext == "dylib") {
                                    let file_name = path.file_name().unwrap();
                                    let dest = target_dir.join(file_name);
                                    let _ = fs::copy(&path, &dest);
                                }
                            }
                        }
                    }
                    println!("✅ [Local-CI] Successfully executed step dynamically from YAML.");
                }
            }
        }
    }

    // Only pass --features flag if driver is llama (ONNX uses prebuilt ORT DLLs via ORT_STRATEGY)
    if driver == "llama" && !matched_features.is_empty() {
        vec!["--features".to_string(), matched_features]
    } else {
        Vec::new()
    }
}
