use std::path::Path;
use anyhow::{Result, anyhow};
use libloading::{Library, Symbol};

pub struct SafetyChecker;

impl SafetyChecker {
    /// 🛡️ The 4-Step Audit: Verifies any plugin/extension before it can be registered or executed by the Engine.
    pub fn audit_plugin(_manifest_path: &Path, binary_path: &Path, manifest: &serde_json::Value) -> Result<()> {
        tracing::info!("🛡️ [SafetyChecker] Starting 4-Step Audit for plugin at {:?}", binary_path);
        
        Self::step1_structural_analysis(manifest, binary_path)?;
        Self::step2_lexical_cel_validation(manifest)?;
        Self::step3_ffi_symbol_resolution(binary_path)?;
        Self::step4_sandbox_enforcement(manifest)?;
        
        tracing::info!("✅ [SafetyChecker] Audit passed securely.");
        Ok(())
    }

    /// Step 1: Structural Analysis & Path Containment
    fn step1_structural_analysis(manifest: &serde_json::Value, binary_path: &Path) -> Result<()> {
        let storage_domain = manifest["storage_domain"].as_str().unwrap_or("extensions");
        
        if storage_domain.starts_with("core/") {
            tracing::warn!("⚠️ [SafetyChecker] Plugin requesting 'core/' domain. High privilege.");
        }
        
        // Path Traversal Check
        let path_str = binary_path.to_string_lossy();
        if path_str.contains("..") {
            return Err(anyhow!("Path traversal detected in binary path. Rejecting plugin for safety."));
        }
        
        Ok(())
    }

    /// Step 2: Lexical CEL Validation
    fn step2_lexical_cel_validation(manifest: &serde_json::Value) -> Result<()> {
        // Ensures the plugin's cel_syntax doesn't hijack engine core keywords
        if let Some(syntax) = manifest["ai_interface"]["cel_syntax"].as_str() {
            let reserved = ["system", "kernel", "memory_free", "reboot", "panic"];
            for r in reserved.iter() {
                if syntax.contains(r) {
                    return Err(anyhow!("Lexical Violation: Plugin attempts to hijack reserved CEL keyword '{}'", r));
                }
            }
        }
        Ok(())
    }

    /// Step 3: FFI Symbol Resolution
    fn step3_ffi_symbol_resolution(binary_path: &Path) -> Result<()> {
        let ext = binary_path.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        if ext == "dll" || ext == "so" || ext == "dylib" {
            // Unsafe block is strictly contained. We only load and check for the symbol, we NEVER execute it here.
            unsafe {
                let lib_result = Library::new(binary_path);
                match lib_result {
                    Ok(lib) => {
                        // Check for mandatory entry point
                        let func: Result<Symbol<unsafe extern "C" fn()>, _> = lib.get(b"execute_cel\0");
                        if func.is_err() {
                            return Err(anyhow!("FFI Symbol Resolution Failed: Mandatory 'execute_cel' C-ABI entry point not found. Engine crash averted."));
                        }
                    },
                    Err(e) => {
                        return Err(anyhow!("FFI Load Failed: Native library could not be loaded safely. {}", e));
                    }
                }
            }
        }
        Ok(())
    }

    /// Step 4: Sandbox & Resource Limits Enforcement
    fn step4_sandbox_enforcement(manifest: &serde_json::Value) -> Result<()> {
        // Check maximum memory allowed
        if let Some(mem_limit) = manifest["engine_rules"]["max_memory_mb"].as_u64() {
            if mem_limit > 4096 {
                return Err(anyhow!("Sandbox limit exceeded: Plugin requests more than 4GB RAM."));
            }
        }
        
        // Check network access flags
        if let Some(network) = manifest["engine_rules"]["allow_network"].as_bool() {
            if network {
                tracing::info!("🌐 [SafetyChecker] Plugin requires network access. Port bindings will be restricted.");
            }
        }
        
        Ok(())
    }
}
