//! 🏠 Local Bridge: Local memory-mapped LMDB database FFI bridge.
//! Leverages the dynamically loaded cluaizd_engine.dll to bypass static linking.

use super::storage_bridge::CognitiveStorageBridge;
use libloading::{Library, Symbol};
use std::ffi::CString;
use std::sync::Arc;

pub struct LocalBridge {
    _lib: Arc<Library>,
    inject_context_fn: Symbol<'static, unsafe extern "C" fn(*const std::ffi::c_char, *mut usize) -> *mut u8>,
    save_context_fn: Symbol<'static, unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, *const f32, usize) -> i32>,
    free_payload_fn: Symbol<'static, unsafe extern "C" fn(*mut u8, usize)>,
}

impl LocalBridge {
    pub fn new() -> Self {
        unsafe {
            // Load the true Hub Extension DLL dynamically
            let lib_path = cluaize_shared::hardware::governor::HardwareGovernor::resolve_hub_path()
                .join("extensions/cluaize-db/native/target/release/cluaizd_engine.dll");
                
            tracing::info!("🔗 Loading dynamic Neural Database extension from {:?}", lib_path);
            
            let lib = Library::new(&lib_path).expect("🚨 CRITICAL: Failed to load cluaizd_engine.dll. Is the Hub extension built?");
            
            // Map the FFI Functions
            let boot_env_fn: Symbol<unsafe extern "C" fn() -> i32> = lib.get(b"boot_environment\0").expect("Missing boot_environment in DLL");
            
            if boot_env_fn() != 0 {
                tracing::warn!("⚠️ Warning: boot_environment returned non-zero status.");
            }

            let inject_context_fn: Symbol<unsafe extern "C" fn(*const std::ffi::c_char, *mut usize) -> *mut u8> = lib.get(b"inject_context\0").expect("Missing inject_context");
            let save_context_fn: Symbol<unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, *const f32, usize) -> i32> = lib.get(b"save_context\0").expect("Missing save_context");
            let free_payload_fn: Symbol<unsafe extern "C" fn(*mut u8, usize)> = lib.get(b"free_payload\0").expect("Missing free_payload");

            // Store symbols bounded to 'static lifetime using transmute,
            // relying on the Arc<Library> keeping the DLL loaded for the lifetime of LocalBridge.
            let inject_context_fn = std::mem::transmute(inject_context_fn);
            let save_context_fn = std::mem::transmute(save_context_fn);
            let free_payload_fn = std::mem::transmute(free_payload_fn);

            LocalBridge {
                _lib: Arc::new(lib),
                inject_context_fn,
                save_context_fn,
                free_payload_fn,
            }
        }
    }
}

impl CognitiveStorageBridge for LocalBridge {
    fn inject_context(&self, memory_key: &str) -> Option<Vec<u8>> {
        let key_cstr = CString::new(memory_key).ok()?;
        let mut out_len: usize = 0;
        
        let ptr = unsafe { (self.inject_context_fn)(key_cstr.as_ptr(), &mut out_len) };
        if ptr.is_null() || out_len == 0 {
            return None;
        }

        let slice = unsafe { std::slice::from_raw_parts(ptr, out_len) };
        let vec_data = slice.to_vec();
        
        unsafe { (self.free_payload_fn)(ptr, out_len) };

        Some(vec_data)
    }

    fn save_context(&self, memory_id: &str, payload: &str, vector: &[f32]) -> Result<(), String> {
        let id_cstr = CString::new(memory_id).map_err(|e| e.to_string())?;
        let payload_cstr = CString::new(payload).map_err(|e| e.to_string())?;
        
        let result = unsafe {
            (self.save_context_fn)(
                id_cstr.as_ptr(),
                payload_cstr.as_ptr(),
                vector.as_ptr(),
                vector.len(),
            )
        };

        if result != 0 {
            Err(format!("FFI parameterized insertion failed with error code: {}", result))
        } else {
            Ok(())
        }
    }
}
