use std::ffi::{c_char, CStr, CString};
use libloading::{Library, Symbol};
use crate::ffi::cxp_ffi::ExtensionPayload;

/// Executor for dynamically loading and running native (`.dll` / `.so`) plugins.
/// Ensures the Engine remains a 'Dumb Router'.
pub struct NativeExecutor {
    // Optionally hold a registry or cache of loaded libraries
}

impl Default for NativeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self {}
    }

    /// Loads a native plugin DLL and passes the CXP `ExtensionPayload` pointer.
    pub fn execute(&self, plugin_path: &str, payload: &ExtensionPayload) -> Result<Vec<u8>, String> {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            return Err("Dynamic Loading (C-FFI) is banned on Mobile OS (iOS/Android) due to security policies. Native plugins must be statically linked.".to_string());
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        unsafe {
            // 1. Load the Universal Plugin DLL
            let lib = Library::new(plugin_path)
                .map_err(|e| format!("Failed to load native plugin '{}': {}", plugin_path, e))?;

            // 2. Find the Universal CEL Execution Function
            let execute_cel: Symbol<unsafe extern "C" fn(*const ExtensionPayload) -> *mut c_char> = 
                lib.get(b"execute_cel\0")
                   .map_err(|e| format!("Symbol 'execute_cel' not found in plugin: {}", e))?;

            // 3. Execute the Native Plugin at zero-cost abstraction
            let result_ptr = execute_cel(payload as *const ExtensionPayload);

            if result_ptr.is_null() {
                return Err("Native plugin returned null pointer".to_string());
            }

            // 4. Extract result and free memory (using standard C string rules for now)
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_bytes = result_cstr.to_bytes().to_vec();

            // Note: In a production environment, you need an exported `free_memory` function
            // to avoid leaking memory allocated by the DLL. For this MVP, we assume the DLL handles it 
            // or we will add a `cluaiz_free_ptr` symbol later.

            Ok(result_bytes)
        }
    }
}
