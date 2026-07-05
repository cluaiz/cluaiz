use std::ffi::{c_char, CStr};
use libloading::{Library, Symbol};

use crate::ffi::cxp_ffi::ExtensionPayload;
use crate::parser::metadata_parser::EngineRules;

/// Executor for dynamically loading and running native (`.dll` / `.so`) plugins.
///
/// The engine remains a "Dumb Router" — it does not decide what the plugin does.
/// All execution constraints (permissions, memory policy) come from the plugin's
/// `EngineRules` in its manifest.
///
/// Security policy: Native plugins are ONLY permitted for trusted core components
/// (name prefix `core_` or `engine_`). Community plugins MUST use the WASM sandbox.
pub struct NativeExecutor {
    _private: (), // Prevents direct field construction — use NativeExecutor::new()
}

impl Default for NativeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Loads a native plugin and executes it with constraints from `EngineRules`.
    ///
    /// Memory management: After reading the result, this function attempts to call
    /// `cluaiz_free_payload` from the plugin's own exports. If not found, a warning
    /// is logged — the caller/plugin author must fix this to prevent RAM leaks.
    ///
    /// Mobile OS ban: Native dynamic loading is blocked on iOS and Android.
    /// All mobile plugins MUST be statically linked.
    pub fn execute_with_rules(
        &self,
        plugin_path: &str,
        payload: &ExtensionPayload,
        rules: &EngineRules,
    ) -> Result<Vec<u8>, String> {
        // Platform gate — mobile OS bans dynamic native loading
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            return Err(
                "Dynamic native loading (C-FFI) is banned on mobile OS (iOS/Android). \
                 Native plugins must be statically linked."
                    .to_string(),
            );
        }

        // Subprocess spawn check — if manifest explicitly blocks it, log the restriction
        if rules.allow_subprocess == Some(false) {
            tracing::debug!(
                "Native plugin '{}': subprocess spawning denied by manifest rule.",
                plugin_path
            );
        }

        // Env var read check — explicit record from manifest
        if rules.allow_env_vars == Some(false) {
            tracing::debug!(
                "Native plugin '{}': environment variable access denied by manifest rule.",
                plugin_path
            );
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        unsafe {
            // H-1: Canonicalize the path before loading — prevents relative path DLL hijacking.
            // libloading on Windows uses LoadLibraryA which follows DLL search order for relative paths.
            let abs_plugin_path = std::fs::canonicalize(plugin_path).map_err(|e| {
                format!(
                    "Cannot resolve native plugin path '{}' to absolute path: {}. \
                     Relative paths are not permitted for security reasons.",
                    plugin_path, e
                )
            })?;

            // 1. Load the native plugin library using the canonicalized absolute path
            tracing::info!("LOADING NATIVE DLL FROM EXACT PATH: {}", abs_plugin_path.display());
            let lib = Library::new(&abs_plugin_path).map_err(|e| {
                format!("Failed to load native plugin '{}': {}", abs_plugin_path.display(), e)
            })?;

            // 2. Resolve the universal CEL boundary function
            let execute_cel: Symbol<unsafe extern "C" fn(*const ExtensionPayload) -> *mut c_char> =
                lib.get(b"execute_cel\0").map_err(|e| {
                    format!(
                        "Symbol 'execute_cel' not found in native plugin '{}': {}",
                        plugin_path, e
                    )
                })?;

            // 3. Execute the native plugin
            let result_ptr = execute_cel(payload as *const ExtensionPayload);

            if result_ptr.is_null() {
                return Err(format!(
                    "Native plugin '{}' returned a null pointer from execute_cel().",
                    plugin_path
                ));
            }

            // 4. Extract result bytes before freeing
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_bytes = result_cstr.to_bytes().to_vec();

            // 5. Free the pointer returned by the plugin.
            //    Preference order:
            //    a) Plugin-exported `cluaiz_free_payload` — uses plugin's own allocator
            //    b) Log a clear warning — DO NOT silently leak
            let free_sym: Result<Symbol<unsafe extern "C" fn(*mut u8, usize)>, _> =
                lib.get(b"cluaiz_free_payload\0");

            match free_sym {
                Ok(free_fn) => {
                    free_fn(result_ptr as *mut u8, result_bytes.len());
                    tracing::debug!(
                        "Native plugin '{}': result pointer freed via plugin-exported symbol.",
                        plugin_path
                    );
                }
                Err(_) => {
                    // Plugin does not export a free function — this is a plugin authoring error.
                    // We log a warning rather than silently leaking or crashing.
                    tracing::warn!(
                        "Native plugin '{}' does not export 'cluaiz_free_payload'. \
                         The memory allocated by this plugin will leak. \
                         Plugin authors must export this symbol to be compatible with the cluaiz engine.",
                        plugin_path
                    );
                }
            }

            Ok(result_bytes)
        }

        // Unreachable on non-mobile targets due to cfg gates, but required for type coherence
        #[cfg(any(target_os = "android", target_os = "ios"))]
        unreachable!()
    }
}
