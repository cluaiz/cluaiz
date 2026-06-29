//! Auto-WASM Background Compiler
//! As defined in the Phase 2 research, this component is responsible for taking raw Rust scripts 
//! (e.g. `logic.rs`), compiling them dynamically into WASM format in the background, 
//! and hot-reloading them into the Engine's RAM Sandbox without requiring an Engine reboot.

use std::path::Path;
use std::process::Command;
use std::fs;

pub struct AutoWasmCompiler;

impl Default for AutoWasmCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoWasmCompiler {
    pub fn new() -> Self {
        Self {}
    }

    /// Takes raw Rust source code as a string, invokes the WASM compiler (`cargo build --target wasm32-unknown-unknown`),
    /// and returns the compiled binary `.wasm` byte vector for hot-reloading.
    pub fn compile_rust_to_wasm(&self, script_code: &str) -> Result<Vec<u8>, String> {
        let temp_dir = std::env::temp_dir().join(format!("cluaiz_auto_wasm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

        let cargo_toml = format!(r#"
[package]
name = "dynamic_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
        "#);

        fs::write(temp_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
        
        let src_dir = temp_dir.join("src");
        fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
        fs::write(src_dir.join("lib.rs"), script_code).map_err(|e| e.to_string())?;

        let output = Command::new("cargo")
            .current_dir(&temp_dir)
            .args(&["build", "--release", "--target", "wasm32-unknown-unknown"])
            .output()
            .map_err(|e| format!("Failed to invoke Cargo: {}", e))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!("Auto-WASM Compilation failed: {}", error_msg));
        }

        let wasm_path = temp_dir.join("target/wasm32-unknown-unknown/release/dynamic_plugin.wasm");
        let wasm_bytes = fs::read(&wasm_path).map_err(|e| format!("Failed to read compiled WASM: {}", e))?;

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(wasm_bytes)
    }

    /// Compiles a `.rs` file dynamically and saves the output to a specified WASM path.
    pub fn compile_file(&self, source_path: &Path, output_wasm_path: &Path) -> Result<(), String> {
        tracing::info!("Auto-WASM Triggered: Preparing to dynamically compile {:?}", source_path);
        let script_code = fs::read_to_string(source_path).map_err(|e| e.to_string())?;
        let bytes = self.compile_rust_to_wasm(&script_code)?;
        fs::write(output_wasm_path, bytes).map_err(|e| e.to_string())?;
        Ok(())
    }
}
