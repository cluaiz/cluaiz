//! bridge.rs: The Neural Soul Linker (Internalized).
//! This is an internal driver for archer-llama to handle specialized 1-bit (Bonsai) tensors.

use std::ffi::{CString, CStr};
use std::os::raw::{c_char, c_void};
use libloading::{Library, Symbol};
use std::path::PathBuf;

type CreateBackendFn = unsafe extern "C" fn(path: *const c_char) -> *mut c_void;
type GenerateFn = unsafe extern "C" fn(backend: *mut c_void, prompt: *const c_char, max_tokens: usize) -> *mut c_char;
type FreeStringFn = unsafe extern "C" fn(s: *mut c_char);
type DestroyBackendFn = unsafe extern "C" fn(backend: *mut c_void);

pub struct InternalPrismBridge {
    _library: Library,
    backend_ptr: *mut c_void,
    fn_generate: GenerateFn,
    fn_free_string: FreeStringFn,
    fn_destroy_backend: DestroyBackendFn,
}

impl InternalPrismBridge {
    pub fn load_specialized(model_path: &str) -> std::result::Result<Self, String> {
        // 🛡️ Sovereign Path Resolution: No hardcoded drives, fully dynamic.
        let lib_name = if cfg!(windows) {
            "archer_prism.dll"
        } else {
            "libarcher_prism.so"
        };

        // Search strategy:
        // 1. Explicit Environment Variable
        // 2. Relative to current executable
        // 3. Current Working Directory / interface-engines
        let mut lib_path = PathBuf::from(lib_name);
        
        if let Ok(env_path) = std::env::var("ARCHER_PRISM_PATH") {
            lib_path = PathBuf::from(env_path).join(lib_name);
        } else {
            // 🚀 FIX: Use dynamic router resolution instead of fragile hardcoded relative paths
            let base_path = crate::router::BinaryRouter::resolve_binary().parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let candidate = base_path.join(lib_name);
            if candidate.exists() {
                lib_path = candidate;
            }
        }

        if !lib_path.exists() {
            return Err(format!("❌ Prism-Inference Kernel not found for {}. Please set ARCHER_PRISM_PATH or ensure the DLL is placed alongside the dynamic hardware binary.", lib_name));
        }

        unsafe {
            let lib = Library::new(&lib_path).map_err(|e| format!("Failed to load Prism library at {:?}: {}", lib_path, e))?;
            
            let create_fn: Symbol<CreateBackendFn> = lib.get(b"create_backend\0")
                .map_err(|_| "Missing symbol: create_backend")?;
            
            let fn_generate: Symbol<GenerateFn> = lib.get(b"backend_generate\0")
                .map_err(|_| "Missing symbol: backend_generate")?;
                
            let fn_free_string: Symbol<FreeStringFn> = lib.get(b"free_string\0")
                .map_err(|_| "Missing symbol: free_string")?;
                
            let fn_destroy_backend: Symbol<DestroyBackendFn> = lib.get(b"destroy_backend\0")
                .map_err(|_| "Missing symbol: destroy_backend")?;

            let c_path = CString::new(model_path).map_err(|_| "Invalid model path")?;
            let backend_ptr = create_fn(c_path.as_ptr());

            if backend_ptr.is_null() {
                return Err("Failed to initialize Prism backend instance".to_string());
            }

            Ok(Self {
                fn_generate: *fn_generate,
                fn_free_string: *fn_free_string,
                fn_destroy_backend: *fn_destroy_backend,
                _library: lib,
                backend_ptr,
            })
        }
    }

    pub fn generate(&mut self, prompt: &str, max_tokens: usize) -> std::result::Result<String, String> {
        let c_prompt = CString::new(prompt).map_err(|_| "Invalid prompt string")?;
        unsafe {
            let res_ptr = (self.fn_generate)(self.backend_ptr, c_prompt.as_ptr(), max_tokens);
            if res_ptr.is_null() { return Err("Generation failed".into()); }
            let c_res = CStr::from_ptr(res_ptr);
            let response = c_res.to_string_lossy().into_owned();
            (self.fn_free_string)(res_ptr);
            Ok(response)
        }
    }
}

impl Drop for InternalPrismBridge {
    fn drop(&mut self) {
        unsafe { (self.fn_destroy_backend)(self.backend_ptr); }
    }
}
