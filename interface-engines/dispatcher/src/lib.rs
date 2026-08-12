use anyhow::Result;
use cluaiz_shared::backend::signature::{KernelSignature, GlobalFeatureRegistry, BackendType};
use cluaiz_shared::hardware::schema::optimization::OptimizationControl;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use tokio::sync::mpsc;


pub enum EngineResponse {
    TokenStream(mpsc::Receiver<String>),
    FinalResult(String),
    Error(String),
}

#[derive(Clone)]
pub struct SafeEnginePtr(pub *mut std::ffi::c_void);
unsafe impl Send for SafeEnginePtr {}
unsafe impl Sync for SafeEnginePtr {}

/// NeuralDispatcher (The Master Router)
/// The core router that owns hardware logic and dispatches prompts across Native IPC and HTTP.
pub struct NeuralDispatcher {
    pub opt_state: OptimizationControl,
    pub current_signature: KernelSignature,
    pub cached_engine: std::sync::Arc<tokio::sync::Mutex<Option<(PathBuf, SafeEnginePtr, std::sync::Arc<libloading::Library>)>>>,
    /// Limits concurrent LLM dispatches to prevent system overload (acts as an inference queue)
    pub inference_semaphore: Arc<tokio::sync::Semaphore>,
    /// Per-instance cancellation flag — set to true to stop the active generation
    pub cancel_flag: Arc<AtomicBool>,
}

impl NeuralDispatcher {
    pub fn new(opt_state: OptimizationControl, signature: KernelSignature) -> Self {
        Self {
            opt_state,
            current_signature: signature,
            cached_engine: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            // Max 4 concurrent LLM generations — extras wait in queue
            inference_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Primary entry point for real-time token streaming.
    /// Used by both the FFI Named Pipes (Native Desktop) and HTTP SSE (External).
    pub async fn dispatch_stream(&self, prompt: &str, skip_brain: bool, model_path_opt: Option<PathBuf>) -> EngineResponse {
        // 🚀 Real-time Silicon Probe
        let hardware = cluaiz_shared::hardware::HardwareOrchestrator::probe().silicon_truth;
        let backend = GlobalFeatureRegistry::select_runtime(&self.current_signature, &hardware);
        
        tracing::info!("🚦 [Master Router] Routing prompt to backend: {:?}", backend);

        let (tx, rx) = mpsc::channel::<String>(100);
        let prompt_clone = prompt.to_string();

        match backend {
            BackendType::RuntimeB | BackendType::RuntimeC | BackendType::RuntimeA => {
                let cached_engine_lock = self.cached_engine.clone();
                let semaphore = self.inference_semaphore.clone();
                let cancel_flag = self.cancel_flag.clone();
                // Reset cancellation for this new request
                cancel_flag.store(false, Ordering::Relaxed);
                tokio::spawn(async move {
                    // 🔢 Inference Queue: acquire a slot before proceeding (blocks if 4 already running)
                    let _permit = match semaphore.acquire().await {
                        Ok(permit) => permit,
                        Err(_) => {
                            let _ = tx.send("Error: Inference queue closed.".to_string()).await;
                            return;
                        }
                    };
                    tracing::info!("🔢 [Dispatcher] Inference slot acquired. Running generation...");

                    let active_path = model_path_opt;
                    let model_path = match active_path {
                        Some(ref path) => path.clone(),
                        None => {
                            tracing::error!(
                                "❌ [Dispatcher] No active model configured. \
                                 Check ~/.cluaiz/engine/config/permission.json \
                                 and verify the model directory exists under ~/.cluaiz/models/chat/."
                            );
                            let _ = tx.send(
                                "Error: No active model is configured. \
                                 Please set a model in permission.json or via the /models/load API."
                                    .to_string(),
                            ).await;
                            let _ = tx.send("\n[DONE]\n".to_string()).await;
                            return;
                        }
                    };

                    let mut engine_lock = cached_engine_lock.lock().await;
                    
                    // Check if we need to load a new model
                    let mut load_new = true;
                    if let Some((ref cached_path, ref safe_ptr, ref lib)) = *engine_lock {
                        if cached_path == &model_path && !safe_ptr.0.is_null() {
                            load_new = false;
                        }
                    }

                    if load_new {
                        // Free previous engine if it existed
                        if let Some((cached_path, safe_ptr, lib)) = engine_lock.take() {
                            unsafe {
                                if let Ok(free_fn) = lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void)>(b"cluaiz_kernel_free") {
                                    tracing::info!("🗑️ [Dispatcher] Freeing previous model instance");
                                    free_fn(safe_ptr.0);
                                    
                                    let engine_id = cached_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    cluaiz_shared::HardwareGovernor::unregister_allocation(&engine_id);
                                }
                            }
                            // 🛑 CRITICAL FIX: Leak the library handle so the DLL is never unloaded from memory.
                            // If we drop it, the OS violently unmaps `llama.cpp` while background threads exist, causing STATUS_ACCESS_VIOLATION.
                            Box::leak(Box::new(lib));
                        }

                        // Resolve path and load DLL (dynamic routing for GGUF vs ONNX)
                        let target_os = std::env::consts::OS;
                        let ext = match target_os {
                            "windows" => "dll",
                            "macos" => "dylib",
                            _ => "so",
                        };
                        let prefix = if target_os == "windows" { "" } else { "lib" };

                        let path_str_lower = model_path.to_string_lossy().to_lowercase();
                        let is_onnx = model_path
                            .extension()
                            .map_or(false, |e| e.to_string_lossy().eq_ignore_ascii_case("onnx"))
                            || path_str_lower.contains("onnx");
                        let core_name = if is_onnx { "cluaiz-onnx" } else { "cluaiz-llama" };

                        let binary_name = format!("{}{}.{}", prefix, core_name, ext);

                        let binary_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                            .join(&binary_name);

                        // 🛡️ Strict FFI Validation Boundary
                        let marker_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                            .join(format!("{}.ready", core_name));

                        if !binary_path.exists() || !marker_path.exists() {
                            tracing::error!("❌ [Dispatcher] FFI Validation Failed: Kernel binary or manifest marker missing at {:?}", binary_path);
                            let _ = tx.blocking_send("Error: Missing kernel binary or manifest validation failed.".to_string());
                            let _ = tx.blocking_send("\n[DONE]\n".to_string());
                            return; // Stop loading logic
                        }

                        tracing::info!("🔗 [Dispatcher] Loading validated dynamic library {:?}", binary_path);

                        let mut successfully_loaded = false;
                        unsafe {
                            #[cfg(windows)]
                            let lib = {
                                let flags = 0x00000008; 
                                libloading::os::windows::Library::load_with_flags(&binary_path, flags).ok().map(libloading::Library::from)
                            };

                            #[cfg(not(windows))]
                            let lib = libloading::Library::new(&binary_path).ok();

                            if let Some(library) = lib {
                                let library_arc = std::sync::Arc::new(library);
                                
                                if let Ok(init_fn) = library_arc.get::<unsafe extern "C" fn() -> *const std::os::raw::c_char>(b"cluaiz_kernel_init") {
                                    tracing::info!("🔗 [Dispatcher] Initializing LLM Kernel Backend...");
                                    unsafe { init_fn(); }
                                }

                                if let Ok(instantiate_fn) = library_arc.get::<unsafe extern "C" fn(*const std::os::raw::c_char, *const std::ffi::c_void) -> *mut std::ffi::c_void>(b"cluaiz_kernel_instantiate") {
                                    let c_path = std::ffi::CString::new(model_path.to_string_lossy().to_string()).unwrap();
                                    tracing::info!("🔗 [Dispatcher] Instantiating kernel with model path: {:?}", model_path);
                                    
                                    let instantiate_raw = *instantiate_fn as usize;
                                    let c_path_raw = c_path.as_ptr() as usize;
                                    
                                    let engine_ptr_raw = tokio::task::spawn_blocking(move || {
                                        let func: unsafe extern "C" fn(*const std::os::raw::c_char, *const std::ffi::c_void) -> *mut std::ffi::c_void = unsafe { std::mem::transmute(instantiate_raw) };
                                        let ptr = unsafe { func(c_path_raw as *const _, std::ptr::null()) };
                                        ptr as usize
                                    }).await.unwrap_or(0);
                                    
                                    let engine_ptr = engine_ptr_raw as *mut std::ffi::c_void;
                                    
                                    if !engine_ptr.is_null() {
                                        *engine_lock = Some((model_path.clone(), SafeEnginePtr(engine_ptr), library_arc));
                                        successfully_loaded = true;
                                        
                                        let vram_gb = std::fs::metadata(&model_path)
                                            .map(|m| m.len() as f64 / (1024.0 * 1024.0 * 1024.0))
                                            .unwrap_or(0.0);

                                        if !is_onnx {
                                            let mut dynamic_ctx = 0;
                                            let mut dna = cluaiz_shared::metadata::dna::StructuralDNA::default();
                                            if let Some(parent) = model_path.parent() {
                                                if dna.discover_from_path(parent).is_ok() {
                                                    dynamic_ctx = cluaiz_shared::hardware::governor::HardwareGovernor::negotiate_vram_envelope(&dna);
                                                }
                                            }

                                            cluaiz_shared::HardwareGovernor::register_allocation(
                                                &model_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                                                vram_gb,
                                                dynamic_ctx,
                                                "Native Llama"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        if !successfully_loaded {
                            tracing::error!("❌ [Dispatcher] Failed to load or instantiate LLM engine.");
                        }
                    }

                    // Run generation on the cached/loaded engine
                    let mut generated = false;
                    if let Some((_, ref safe_ptr, ref lib)) = *engine_lock {
                        unsafe {
                            if let Ok(gen_stream_fn) = lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, usize, extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool, *mut std::ffi::c_void)>(b"cluaiz_kernel_generate_stream") {
                                tracing::info!("⏱️ [Dispatcher] Starting generate_stream execution");
                                let c_prompt = std::ffi::CString::new(prompt_clone).unwrap();

                                // 🛑 CANCELLATION-AWARE CALLBACK
                                // user_data carries (tx, cancel_flag, buffer) packed as raw ptr.
                                struct CallbackData {
                                    tx: tokio::sync::mpsc::Sender<String>,
                                    cancel_flag: Arc<AtomicBool>,
                                    buffer: std::sync::Mutex<String>,
                                    first_token_time: std::sync::Mutex<Option<std::time::Instant>>,
                                }

                                extern "C" fn callback(token_ptr: *const std::os::raw::c_char, user_data: *mut std::ffi::c_void) -> bool {
                                    let data = unsafe { &*(user_data as *const CallbackData) };
                                    
                                    if data.cancel_flag.load(Ordering::Relaxed) {
                                        tracing::info!("🛑 [Dispatcher] Inference cancelled via cancel_flag.");
                                        return false;
                                    }
                                    
                                    let mut first_lock = data.first_token_time.lock().unwrap();
                                    if first_lock.is_none() {
                                        *first_lock = Some(std::time::Instant::now());
                                        tracing::info!("⚡ [Dispatcher] TTFT recorded!");
                                    }
                                    
                                    let token = unsafe { std::ffi::CStr::from_ptr(token_ptr) }.to_string_lossy().into_owned();
                                    
                                    // 🚀 Two-Step Discovery: Token Interception Buffer
                                    let mut should_send = true;
                                    if let Ok(mut buffer) = data.buffer.lock() {
                                        buffer.push_str(&token);
                                        
                                        // Are we currently inside a trigger generation?
                                        if let Some(start_idx) = buffer.find("<TRIGGER:") {
                                            should_send = false; // Hide from UI
                                            
                                            // Have we reached the end of the payload?
                                            if let Some(end_idx) = buffer.find("</TRIGGER>") {
                                                // Include the length of </TRIGGER> (10 chars)
                                                let full_trigger = &buffer[start_idx..end_idx + 10];
                                                tracing::info!("🔍 [Dispatcher] Sovereign Interceptor Complete Payload: {}", full_trigger);
                                                let _ = data.tx.blocking_send(full_trigger.to_string());
                                                data.cancel_flag.store(true, Ordering::Relaxed);
                                                return false; // Abort C-FFI Stream gracefully
                                            }
                                        } else {
                                            // Sliding window for performance if we are not inside a trigger
                                            if buffer.len() > 100 {
                                                let mut start_idx = buffer.len() - 100;
                                                while start_idx < buffer.len() && !buffer.is_char_boundary(start_idx) {
                                                    start_idx += 1;
                                                }
                                                *buffer = buffer[start_idx..].to_string();
                                            }
                                        }
                                    }

                                    if should_send {
                                        data.tx.blocking_send(token).is_ok()
                                    } else {
                                        true
                                    }
                                }

                                let callback_data = CallbackData { 
                                    tx: tx.clone(), 
                                    cancel_flag: cancel_flag.clone(),
                                    buffer: std::sync::Mutex::new(String::new()),
                                    first_token_time: std::sync::Mutex::new(None),
                                };
                                let tx_ptr = &callback_data as *const CallbackData as *mut std::ffi::c_void;
                                let engine_raw = safe_ptr.0 as usize;
                                let tx_raw = tx_ptr as usize;
                                let cb_raw = callback as usize;
                                let gen_raw = *gen_stream_fn as usize;

                                // 🛡️ FFI PANIC BOUNDARY & BLOCKING THREAD POOL
                                // Offload heavy FFI execution and prevent blocking the async executor.
                                let result = tokio::task::spawn_blocking(move || {
                                    // Move c_prompt into the closure so it doesn't get dropped if the async task is cancelled
                                    let _owned_prompt = c_prompt;
                                    let prompt_raw_ptr = _owned_prompt.as_ptr();
                                    
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        let callback_fn: extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool = unsafe { std::mem::transmute(cb_raw) };
                                        let gen_fn: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, usize, extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool, *mut std::ffi::c_void) = unsafe { std::mem::transmute(gen_raw) };
                                        unsafe {
                                            gen_fn(engine_raw as *mut _, prompt_raw_ptr as *const _, 4096, callback_fn, tx_raw as *mut _);
                                        }
                                    }))
                                }).await.unwrap_or_else(|_| Err(Box::new("Thread join error")));

                                if let Err(panic_payload) = result {
                                    let msg = panic_payload
                                        .downcast_ref::<&str>()
                                        .copied()
                                        .unwrap_or("unknown FFI panic");
                                    tracing::error!("💥 [Dispatcher] FFI Panic caught at generate_stream boundary: {}", msg);
                                    let _ = tx.send(format!("Error: FFI kernel panicked — {}", msg)).await;
                                }

                                // 🚀 Two-Step Discovery: Final Buffer Check
                                // If the LLM stopped generation naturally right after emitting the trigger name
                                // (without a trailing non-alphanumeric character), it won't be caught by the callback loop.
                                // We check the final buffer state here before sending `[DONE]`.
                                if !cancel_flag.load(Ordering::Relaxed) {
                                    let mut intercepted_trigger = None;
                                    if let Ok(buffer) = callback_data.buffer.lock() {
                                        if let Some(start_idx) = buffer.find("<TRIGGER:") {
                                            if let Some(end_idx) = buffer.find("</TRIGGER>") {
                                                let full_trigger = &buffer[start_idx..end_idx + 10];
                                                intercepted_trigger = Some(full_trigger.to_string());
                                                tracing::info!("🔍 [Dispatcher] Two-Step Discovery Complete Payload for: {}", full_trigger);
                                            } else {
                                                // Fallback if the model abruptly ended without closing
                                                let full_trigger = &buffer[start_idx..];
                                                intercepted_trigger = Some(full_trigger.to_string());
                                            }
                                        }
                                    }
                                    
                                    if let Some(trigger_msg) = intercepted_trigger {
                                        let _ = tx.send(trigger_msg).await;
                                    }
                                }

                                generated = true;
                            }
                        }
                    }

                    if !generated {
                        let _ = tx.send("Error: FFI Kernel not active.".to_string()).await;
                    }

                    let _ = tx.send("\n[DONE]\n".to_string()).await;
                });
                EngineResponse::TokenStream(rx)
            }
            _ => {
                EngineResponse::Error(format!("Unsupported backend architecture: {:?}", backend))
            }
        }
    }

    /// Legacy blocking call, to be deprecated once all clients shift to `dispatch_stream`.
    pub async fn dispatch_prompt(&self, prompt: &str, model_path_opt: Option<PathBuf>) -> Result<String> {
        let mut stream = match self.dispatch_stream(prompt, false, model_path_opt).await {
            EngineResponse::TokenStream(rx) => rx,
            EngineResponse::Error(e) => return Err(anyhow::anyhow!(e)),
            EngineResponse::FinalResult(r) => return Ok(r),
        };
        
        let mut final_text = String::new();
        while let Some(token) = stream.recv().await {
            if token.trim() == "[DONE]" { break; }
            final_text.push_str(&token);
        }
        Ok(final_text)
    }

    /// Unloads the currently active LLM from VRAM.
    pub async fn unload_model(&self) -> Result<(), String> {
        let mut engine_lock = self.cached_engine.lock().await;
        if let Some((_, safe_ptr, lib)) = engine_lock.take() {
            unsafe {
                if let Ok(free_fn) = lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void)>(b"cluaiz_kernel_free") {
                    tracing::info!("🗑️ [Dispatcher] Freeing model instance via explicit unload request");
                    free_fn(safe_ptr.0);
                }
            }
            Box::leak(Box::new(lib));
            Ok(())
        } else {
            Ok(())
        }
    }
}

pub struct EmbeddingDispatcher {
    cached_engine: std::sync::Arc<std::sync::Mutex<Option<(PathBuf, SafeEnginePtr, std::sync::Arc<libloading::Library>)>>>,
}

unsafe impl Send for EmbeddingDispatcher {}
unsafe impl Sync for EmbeddingDispatcher {}

impl EmbeddingDispatcher {
    pub fn new(_format_type: Option<String>) -> Result<Self> {
        Ok(Self {
            cached_engine: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn dispatch_embedding_with_model(&self, text: &str, model_path: &std::path::Path) -> Result<Vec<f32>> {
        let mut engine_lock = self.cached_engine.lock().unwrap();

        let is_gguf = model_path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("gguf")).unwrap_or(false);
        let core_name = if is_gguf { "cluaiz-llama" } else { "cluaiz-onnx" };

        let mut need_load = true;
        if let Some((ref cached_path, ref safe_ptr, ref _lib)) = *engine_lock {
            if cached_path == model_path && !safe_ptr.0.is_null() {
                need_load = false;
            }
        }

        if need_load {
            // Free old engine pointer if one was previously loaded
            if let Some((_, ref safe_ptr, ref lib)) = *engine_lock {
                if !safe_ptr.0.is_null() {
                    unsafe {
                        if let Ok(free_fn) = lib.get::<libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)>>(b"cluaiz_kernel_free") {
                            free_fn(safe_ptr.0);
                        }
                    }
                }
            }

            let target_os = std::env::consts::OS;
            let ext = match target_os {
                "windows" => "dll",
                "macos" => "dylib",
                _ => "so",
            };
            let prefix = if target_os == "windows" { "" } else { "lib" };
            let binary_name = format!("{}{}.{}", prefix, core_name, ext);

            let binary_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                .join(&binary_name);
            let marker_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                .join(format!("{}.ready", core_name));

            if !binary_path.exists() || !marker_path.exists() {
                return Err(anyhow::anyhow!("FFI Validation Failed: Kernel binary or manifest missing at {:?}", binary_path));
            }

            unsafe {
                #[cfg(windows)]
                let lib: libloading::Library = {
                    let drivers_dir = cluaiz_shared::HardwareGovernor::resolve_interface_path().join("drivers");
                    if let Ok(path) = std::env::var("PATH") {
                        std::env::set_var("PATH", format!("{};{}", drivers_dir.display(), path));
                    }
                    let flags = 0x00000008;
                    let win_lib = libloading::os::windows::Library::load_with_flags(&binary_path, flags)
                        .map_err(|e| anyhow::anyhow!("ONNX Binary Mapping Failed on path {:?}: {}. OS Error: {:?}", binary_path, e, std::io::Error::last_os_error()))?;
                    win_lib.into()
                };

                #[cfg(not(windows))]
                let lib = libloading::Library::new(&binary_path)
                    .map_err(|e| anyhow::anyhow!("ONNX Binary Mapping Failed on path {:?}: {}. OS Error: {:?}", binary_path, e, std::io::Error::last_os_error()))?;

                let init: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> = lib.get(b"cluaiz_kernel_init")
                    .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_init' missing"))?;
                init();

                let instantiate_fn: libloading::Symbol<unsafe extern "C" fn(*const std::os::raw::c_char, *const std::ffi::c_void) -> *mut std::ffi::c_void> = 
                    lib.get(b"cluaiz_kernel_instantiate")
                    .map_err(|_| anyhow::anyhow!("Invalid Kernel: 'cluaiz_kernel_instantiate' missing"))?;

                let c_path = std::ffi::CString::new(model_path.to_string_lossy().as_ref())?;
                let engine_ptr = instantiate_fn(c_path.as_ptr() as *const std::os::raw::c_char, std::ptr::null());

                if engine_ptr.is_null() {
                    return Err(anyhow::anyhow!("Kernel Instantiation Failed for model {:?}", model_path));
                }

                tracing::info!("✅ [Dispatcher] {} Kernel Dynamically Linked for Model: {:?}", core_name, model_path);
                let lib_arc = std::sync::Arc::new(lib);
                *engine_lock = Some((model_path.to_path_buf(), SafeEnginePtr(engine_ptr), lib_arc));
            }
        }

        let (_path, safe_ptr, lib) = engine_lock.as_ref().unwrap();
        let engine_ptr = safe_ptr.0;

        unsafe {
            let gen_emb_fn: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, *mut f32, usize, *mut usize) -> i32> = 
                lib.get(b"cluaiz_kernel_generate_embedding")
                    .map_err(|e| anyhow::anyhow!("Symbol 'cluaiz_kernel_generate_embedding' missing: {:?}", e))?;

            let c_prompt = std::ffi::CString::new(text)
                .map_err(|_| anyhow::anyhow!("CString conversion failed"))?;
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
                return Err(anyhow::anyhow!("Embedding generation error code: {}", status));
            }

            out_buffer.truncate(out_len);
            Ok(out_buffer)
        }
    }

    pub fn dispatch_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let engine_lock = self.cached_engine.lock().unwrap();
        if let Some((ref path, _, _)) = *engine_lock {
            let path_clone = path.clone();
            drop(engine_lock);
            self.dispatch_embedding_with_model(text, &path_clone)
        } else {
            Err(anyhow::anyhow!("No active embedding model loaded. Please specify a model path in the request."))
        }
    }

    pub fn dispatch_multimodal(&self, _bytes: &[u8], _modality: neural_core::interfaces::router_contract::Modality, _instruction: Option<String>) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("Multimodal embedding FFI not implemented yet"))
    }
}

impl neural_core::interfaces::router_contract::EmbeddingDriver for EmbeddingDispatcher {
    fn gen_embedding(&self, text: &str) -> Result<Vec<f32>, neural_core::interfaces::router_contract::EngineError> {
        self.dispatch_embedding(text)
            .map_err(|e| neural_core::interfaces::router_contract::EngineError::EmbeddingFailed(e.to_string()))
    }

    fn gen_multimodal_embedding(&self, _bytes: &[u8], _modality: neural_core::interfaces::router_contract::Modality, _instruction: Option<String>) -> Result<Vec<f32>, neural_core::interfaces::router_contract::EngineError> {
        Err(neural_core::interfaces::router_contract::EngineError::UnsupportedModality("Multimodal FFI not implemented yet".to_string()))
    }
}

impl Drop for EmbeddingDispatcher {
    fn drop(&mut self) {
        let mut lock = self.cached_engine.lock().unwrap();
        if let Some((_, safe_ptr, lib)) = lock.take() {
            if !safe_ptr.0.is_null() {
                unsafe {
                    if let Ok(free_fn) = lib.get::<libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)>>(b"cluaiz_kernel_free") {
                        free_fn(safe_ptr.0);
                    }
                }
            }
        }
    }
}
