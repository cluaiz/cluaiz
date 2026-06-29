//! 🏛️ Silicon Kernel: Universal Binary Router
//! Decouples execution from hardcoded strings using capability signatures.

use std::path::PathBuf;
use cluaiz_shared::hardware::get_silicon_state;

pub struct BinaryRouter;

impl BinaryRouter {
    /// Resolves the absolute path to the optimal Llama binary based on OS and Probed Hardware.
    pub fn resolve_binary() -> PathBuf {
        let mut path = std::env::current_dir().unwrap_or_default();
        while !path.join("interface-engines").exists() && path.parent().is_some() {
            path.pop();
        }

        let os_dir = if cfg!(target_os = "windows") { "windows" } 
                    else if cfg!(target_os = "macos") { "macos" } 
                    else { "linux" };

        let arch = if cfg!(any(target_arch = "aarch64", target_arch = "arm")) {
            "arm64"
        } else {
            "x86_64"
        };

        let bin_name = if cfg!(target_os = "windows") { "cluaiz_llama.exe" } 
                      else { "cluaiz_llama" };

        let profile = get_silicon_state();
        
        let driver_slug = if profile.active_drivers.iter().any(|d| d.driver_id == "CUDA") { "cuda" }
                         else if profile.active_drivers.iter().any(|d| d.driver_id == "METAL") { "metal" }
                         else if profile.active_drivers.iter().any(|d| d.driver_id == "ROCM") { "rocm" }
                         else if profile.active_drivers.iter().any(|d| d.driver_id == "VULKAN") { "vulkan" }
                         else if profile.active_drivers.iter().any(|d| d.driver_id == "OPENCL") { "opencl" }
                         else if profile.active_drivers.iter().any(|d| d.driver_id == "NNAPI") { "nnapi" }
                         else { "cpu" };

        // 🏛️ Sovereign Alignment: interface-engines/llama/os/arch/driver/binary
        let bin_path = path.join("interface-engines")
            .join("llama")
            .join(os_dir)
            .join(arch)
            .join(driver_slug)
            .join(bin_name);

        if !bin_path.exists() {
            tracing::warn!("⚠️ [Router] Hardware-Specific Binary NOT found: {:?}", bin_path);
        }

        bin_path
    }

    /// Generates compute-specific CLI arguments based on model DNA and Hardware profile.
    pub fn get_compute_args(requires_gpu: bool) -> Vec<String> {
        let profile = get_silicon_state();
        let mut args = Vec::new();

        if profile.active_drivers.iter().any(|d| d.driver_id == "CUDA" || d.driver_id == "METAL" || d.driver_id == "ROCM") {
            if requires_gpu {
                args.extend(vec!["-ngl".to_string(), "99".to_string()]);
            } else {
                args.extend(vec!["-ngl".to_string(), "32".to_string()]);
            }
        } else if profile.active_drivers.iter().any(|d| d.driver_id == "OPENVINO" || d.driver_id == "QNN") {
            // Backends that use specific NPU offloading
            args.extend(vec!["-ngl".to_string(), "1".to_string()]); 
        } else {
            args.extend(vec!["-ngl".to_string(), "0".to_string()]);
        }

        args.extend(vec!["-b".to_string(), "1".to_string()]);
        args
    }
}
