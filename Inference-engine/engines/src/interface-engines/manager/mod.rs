use std::path::PathBuf;
use libloading::{Library, Symbol};
use crate::interface_engines::manager::kernel_loader::KernelLoader;
use crate::interface_engines::manager::driver_bridge::DriverBridge;
use cluaiz_shared::hardware::schema::profiles::SystemControl;
use cluaiz_shared::hardware::governor::HardwareGovernor;
use colored::Colorize;

pub mod kernel_loader;
pub mod driver_bridge;
pub mod npu_bridge;
pub mod driver_provisioner;

use driver_provisioner::DriverProvisioner;

/// cluaiz Engine Manager
/// Orchestrates pre-compiled Kernels (BitNet, Llama, Candle) and Hardware Drivers.
pub struct EngineManager {
    kernel_dir: PathBuf,
    loader: KernelLoader,
    bridge: DriverBridge,
    // 🏛️ The Soul Link: Holds the active binary in process memory
    active_lib: Option<Library>,
}

impl EngineManager {
    pub fn new(kernel_dir: PathBuf) -> Self {
        Self {
            kernel_dir: kernel_dir.clone(),
            loader: KernelLoader::new(kernel_dir),
            bridge: DriverBridge::new(),
            active_lib: None,
        }
    }

    /// Handshake: Identify the target Hardware and ensure correct kernel/driver presence.
    pub async fn prepare_engine(&self, engine_type: &str) -> Result<PathBuf, String> {
        let control = HardwareGovernor::load_system_control()
            .map_err(|e| format!("Hardware Config Missing or Corrupt: {}", e))?;

        // 🚀 cluaiz Detection Logic: The Triple Handshake
        let os = control.identity.os_target.to_lowercase();
        let arch = control.identity.architecture.to_lowercase();
        let gpu_vendor = control.silicon_truth.accelerators.gpus.first().map(|g| g.vendor.to_lowercase());
        let has_drivers = !control.silicon_truth.active_drivers.is_empty();

        cluaiz_shared::dev_info!("🎯 Engine Prep: OS={}, Arch={}, GPU={:?}, Drivers={}", os, arch, gpu_vendor, has_drivers);

        // 🧠 Mission 12: Chronicle Core Activity
        // Temporarily commented out due to missing CoreGraph in cluaiz_shared
        // let _ = cluaiz_shared::Core::graph::CoreGraph::chronicle_pulse(
        //     "Hardware Handshake & Engine Preparation",
        //     engine_type,
        //     &format!("OS: {}, GPU: {:?}", os, gpu_vendor)
        // );

        // 🚀 Sovereign Backend Resolution (Driven by Registry)
        let registry = cluaiz_shared::RegistryGovernor::load_registry().unwrap_or_default();
        let suffix = cluaiz_shared::RegistryGovernor::resolve_backend(&control, &registry);
        
        cluaiz_shared::dev_info!("🎯 Engine Prep: OS={}, Arch={}, Backend={}", os, arch, suffix);

        // 🚀 NATIVE PROVISIONING: Ensure silicon drivers exist before linkage
        if suffix != "cpu" {
            let manifest_url = registry["components"]["drivers"]["manifest_url"].as_str().unwrap_or_default();
            if let Err(e) = DriverProvisioner::provision_for_hardware(&suffix, manifest_url).await {
                cluaiz_shared::dev_info!("  {} [PROVISIONER] Silicon Handshake Error: {}", "⚠️".yellow(), e);
            }
        }

        let binary_id = engine_type.to_string();

        let mut target_suffix = suffix.as_str();
        let mut target_binary_id = binary_id.clone();

        // 🚀 cluaiz VRAM Handshake: Pre-Flight Check (Hardware-Aware Routing)
        // Bypassing strict GPU VRAM arbitration for llama.cpp as it natively manages tensor splitting.
        if suffix == "cuda" || suffix == "metal" || suffix == "rocm" || suffix == "vulkan" {
            tracing::info!("🧠 [Arbiter] Hardware Linkage targeting GPU. Bypassing strict VRAM arbitration to allow native Engine management.");
        } else {
            tracing::info!("🧠 [Arbiter] Hardware Linkage targeting CPU/System RAM. Bypassing GPU VRAM limits.");
        }

        // 🎯 [Core Provisioning]: Ensure the specialized kernel binary exists
        let registry_engines_url = registry["components"]["kernel"]["manifest_url"].as_str().unwrap_or_default();
        
        let mut using_base_fallback = false;
        
        let binary_path = if self.loader.exists(&target_binary_id) {
            // 🚀 Try specific kernel first (e.g., cluaiz-llama.dll from DevSync)
            self.loader.resolve_path(&target_binary_id)
        } else {
            // 🚀 ATOMIC PROVISIONING: Attempt to download specialized kernel from registry
            if target_suffix != "cpu" {
                cluaiz_shared::dev_info!("  {} [LINKER] Specialized Kernel '{}' missing. Initiating Sovereign Provisioning...", "🧬".cyan(), target_binary_id);
                match DriverProvisioner::provision_kernel(engine_type, target_suffix, registry_engines_url).await {
                    Ok(path) => path,
                    Err(e) => {
                        tracing::warn!("⚠️ [Provisioner] Kernel provisioning failed ({}). Attempting CPU fallback.", e);
                        target_suffix = "cpu";
                        target_binary_id = engine_type.to_string();
                        self.loader.resolve_path(&target_binary_id)
                    }
                }
            } else {
                self.loader.resolve_path(&target_binary_id)
            }
        };

        if binary_path.exists() {
            Ok(binary_path)
        } else {
            // If binary missing (and no fallback), release the reserved memory immediately
            let _ = HardwareGovernor::release_vram(&target_binary_id);
            Err(format!("Engine Binary Missing: Registry and Fallback both failed for '{}' package.", target_binary_id))
        }
    }

    /// Unload Engine: Release resources back to the cluaiz Governor.
    pub fn release_engine(&self, engine_type: &str) -> anyhow::Result<()> {
        // We need to know which suffix was used to reconstruct the ID
        // For simplicity in V1, we iterate and release what matches the prefix
        HardwareGovernor::release_vram(engine_type)
    }

    /// 🔗 cluaiz Linker: Maps the binary kernel to process memory and resolves symbols.
    pub fn load_and_link(&mut self, binary_path: PathBuf) -> anyhow::Result<()> {
        cluaiz_shared::dev_info!("🧬 [Linker] Mapping binary: {:?}", binary_path);
        tracing::info!("🧬 [Linker] Mapping binary: {:?}", binary_path);
        
        // 🪟 WINDOWS SEARCH PATCH: Add drivers directory to DLL search path
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;
            let discovery_paths = DriverProvisioner::discover_system_paths();
            
            unsafe {
                extern "system" {
                    fn SetDllDirectoryW(lpPathName: *const u16) -> i32;
                }
                
                for path in discovery_paths {
                    if path.exists() {
                        let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
                        path_wide.push(0);
                        SetDllDirectoryW(path_wide.as_ptr());
                    }
                }
            }
        }

        unsafe {
            let lib = Library::new(&binary_path)
                .map_err(|e| anyhow::anyhow!("Binary Mapping Failed (libloading): {}", e))?;
            
            // 🎯 Phase 1: Symbol Validation
            let _init: Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> = lib.get(b"cluaiz_kernel_init")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_init' symbol missing."))?;

            // 🎯 Phase 1.5: Pass the GLOBAL_SKIP_THINKING_SIGNAL pointer if the kernel supports it
            if let Ok(set_skip_ptr_fn) = lib.get::<unsafe extern "C" fn(*const std::sync::atomic::AtomicBool)>(b"cluaiz_kernel_set_skip_ptr") {
                set_skip_ptr_fn(&cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL as *const _);
                tracing::info!("🔗 [Linker] Synchronized Sovereign Skip-Thinking Pointer across FFI.");
            }

            tracing::info!("✅ [Linker] 7ns Handshake Complete. Kernel Linked.");
            self.active_lib = Some(lib);
        }
        
        Ok(())
    }

    /// 🏛️ Core Instantiation: Invokes the kernel's factory method to create an active execution engine.
    pub fn instantiate(&self, model_path: &str, booster: &cluaiz_shared::hardware::schema::booster::BoosterControl, max_context_length: Option<u32>) -> anyhow::Result<*mut std::ffi::c_void> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            let instantiate_fn: Symbol<unsafe extern "C" fn(*const std::os::raw::c_char, *const cluaiz_shared::hardware::schema::booster::cluaizBoosterContext) -> *mut std::ffi::c_void> = 
                lib.get(b"cluaiz_kernel_instantiate")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_instantiate' symbol missing."))?;
            
            let c_path = std::ffi::CString::new(model_path)?;
            let mut booster_ctx: cluaiz_shared::hardware::schema::booster::cluaizBoosterContext = booster.into();
            if let Some(mcl) = max_context_length {
                booster_ctx.max_context_length = mcl;
            }
            let engine_ptr = instantiate_fn(c_path.as_ptr() as *const std::os::raw::c_char, &booster_ctx as *const _);
            
            if engine_ptr.is_null() {
                return Err(anyhow::anyhow!("Kernel Instantiation Failed: Pointer is null. Check kernel logs."));
            }

            tracing::info!("🚀 [Linker] Core Kernel Instantiated at Bare-Metal level.");
            Ok(engine_ptr)
        }
    }

    /// 🌊 [FFI Bridge] Direct token streaming from the linked binary.
    pub fn generate_stream_ffi(
        &self,
        engine_ptr: *mut std::ffi::c_void,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            let generate_fn: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, usize, extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool, *mut std::ffi::c_void) -> i32> = 
                lib.get(b"cluaiz_kernel_generate_stream")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_generate_stream' symbol missing."))?;
            
            let c_prompt = std::ffi::CString::new(prompt)?;
            
            // 🛰️ Static Gateway: Convert the C callback back to our Rust closure
            extern "C" fn c_callback_bridge(token_ptr: *const std::os::raw::c_char, user_data: *mut std::ffi::c_void) -> bool {
                unsafe {
                    let cb = &mut *(user_data as *mut Box<dyn FnMut(String) -> bool + Send + 'static>);
                    let token = std::ffi::CStr::from_ptr(token_ptr).to_string_lossy().into_owned();
                    cb(token)
                }
            }

            // Wrap the callback in another box to keep it alive during the call
            let mut user_data = Box::new(callback);
            let status = generate_fn(
                engine_ptr, 
                c_prompt.as_ptr() as *const std::os::raw::c_char, 
                max_tokens, 
                c_callback_bridge,
                &mut *user_data as *mut _ as *mut std::ffi::c_void
            );
            
            if status != 0 {
                return Err(anyhow::anyhow!("FFI Generation Error (Code: {})", status));
            }
        }
        
        Ok(())
    }

    /// 🚥 [FFI Bridge] Direct embedding generation from the linked binary (used for ONNX).
    pub fn generate_embedding_ffi(
        &self,
        engine_ptr: *mut std::ffi::c_void,
        prompt: &str,
    ) -> anyhow::Result<Vec<f32>> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            let gen_emb_fn: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, *mut f32, usize, *mut usize) -> i32> = 
                lib.get(b"cluaiz_kernel_generate_embedding")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_generate_embedding' symbol missing."))?;
            
            let c_prompt = std::ffi::CString::new(prompt)?;
            
            // Allocate a buffer for the embeddings (e.g., max 8192 dims)
            let max_dims = 8192;
            let mut out_buffer = vec![0.0f32; max_dims];
            let mut out_len: usize = 0;

            let status = gen_emb_fn(
                engine_ptr, 
                c_prompt.as_ptr() as *const std::os::raw::c_char, 
                out_buffer.as_mut_ptr(),
                max_dims,
                &mut out_len as *mut usize
            );
            
            if status != 0 {
                return Err(anyhow::anyhow!("FFI Embedding Generation Error (Code: {})", status));
            }
            
            out_buffer.truncate(out_len);
            Ok(out_buffer)
        }
    }

    /// 💾 [FFI Bridge] Dump the active KV Cache memory state to a safetensors/bin file.
    pub fn dump_kv_cache_ffi(
        &self,
        engine_ptr: *mut std::ffi::c_void,
        path: &str,
    ) -> anyhow::Result<()> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            let dump_fn: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) -> i32> = 
                lib.get(b"cluaiz_kernel_dump_kv_cache")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_dump_kv_cache' symbol missing."))?;
            
            let c_path = std::ffi::CString::new(path)?;
            
            let status = dump_fn(
                engine_ptr, 
                c_path.as_ptr() as *const std::os::raw::c_char
            );
            
            if status != 0 {
                return Err(anyhow::anyhow!("FFI KV Cache Dump Error (Code: {})", status));
            }
        }
        
        Ok(())
    }

    /// 💾 [FFI Bridge] Load KV Cache memory state from a binary file.
    pub fn load_kv_cache_ffi(
        &self,
        engine_ptr: *mut std::ffi::c_void,
        path: &str,
    ) -> anyhow::Result<()> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            let load_fn: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) -> i32> = 
                lib.get(b"cluaiz_kernel_load_kv_cache")
                .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_load_kv_cache' symbol missing."))?;
            
            let c_path = std::ffi::CString::new(path)?;
            
            let status = load_fn(
                engine_ptr, 
                c_path.as_ptr() as *const std::os::raw::c_char
            );
            
            if status != 0 {
                return Err(anyhow::anyhow!("FFI KV Cache Load Error (Code: {})", status));
            }
        }
        
        Ok(())
    }

    /// 💉 [FFI Bridge] Injects pre-computed KV Cache chunks dynamically into the active context prefix.
    pub fn inject_signals_ffi(
        &self,
        engine_ptr: *mut std::ffi::c_void,
        signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>,
    ) -> anyhow::Result<()> {
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            // Check if kernel supports dynamic signal injection
            match lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, usize, usize) -> i32>(b"cluaiz_kernel_inject_signals") {
                Ok(inject_fn) => {
                    for signal in signals {
                        // Assuming raw_data maps to underlying MappedBuffer bytes
                        // and passing length of tokens and head dimensions
                        let raw_bytes_ptr = signal.raw_data.as_ptr();
                        let total_bytes = signal.raw_data.len();
                        
                        let status = inject_fn(engine_ptr, raw_bytes_ptr, total_bytes, signal.token_count);
                        if status != 0 {
                            tracing::warn!("⚠️ [FFI] Kernel failed to inject signal chunk (Code: {})", status);
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!("⚠️ [Linker] 'cluaiz_kernel_inject_signals' symbol missing. Hardware does not support dynamic KV prefixing yet.");
                }
            }
        }
        
        Ok(())
    }

    /// 🏛️ Core Destruction: Invokes the kernel's destructor to free the active execution engine.
    pub fn free_instance(&self, engine_ptr: *mut std::ffi::c_void) -> anyhow::Result<()> {
        if engine_ptr.is_null() {
            return Ok(());
        }
        let lib = self.active_lib.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linker Error: No active kernel linked."))?;
        
        unsafe {
            match lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void)>(b"cluaiz_kernel_free") {
                Ok(free_fn) => {
                    free_fn(engine_ptr);
                    tracing::info!("🗑️ [Linker] Core Kernel Instantiation freed.");
                }
                Err(_) => {
                    tracing::warn!("⚠️ [Linker] 'cluaiz_kernel_free' symbol missing from active kernel. Memory might be leaked.");
                }
            }
            Ok(())
        }
    }

}

impl Drop for EngineManager {
    fn drop(&mut self) {
        // 🏛️ Sovereign FFI Safeguard: Prevent DLL unloading crashes
        // Unloading llama.cpp/CUDA FFI libraries from process memory while global static CUDA
        // or GGML threads are in flight can cause a STATUS_ACCESS_VIOLATION during process exit.
        // We leak the Library handle using std::mem::forget to keep the DLL mapped until the OS
        // cleans up the entire process address space cleanly on exit.
        if let Some(lib) = self.active_lib.take() {
            std::mem::forget(lib);
        }
    }
}
