use anyhow::{Result, anyhow};
use cluaiz_shared::backend::signature::{KernelSignature, GlobalFeatureRegistry, BackendType};
use system_booster::BoosterControl;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use tokio::sync::mpsc;

fn resolve_active_model_path() -> Option<PathBuf> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let local_hub = env.local_dir.clone();
    let global_hub = env.global_dir.clone();
    
    let perm_path = local_hub.join("engine").join("config").join("Permission.json");
    let perm_str = std::fs::read_to_string(&perm_path).ok()?;
    let perm_json: serde_json::Value = serde_json::from_str(&perm_str).ok()?;
    let active_id = perm_json
        .get("chat_models")?
        .get("text")?
        .as_str()?
        .replace(':', "-");
    
    let categories = ["chat", "embedding", "vision", "audio", "code"];
    
    // Check local models root first, then global models root
    let roots = [local_hub.join("models"), global_hub.join("models")];
    
    for models_root in &roots {
        for category in &categories {
            let model_dir = models_root.join(category).join(&active_id);
            if model_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&model_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    None
}

pub enum EngineResponse {
    TokenStream(mpsc::Receiver<String>),
    FinalResult(String),
    Error(String),
}

#[derive(Clone)]
pub struct SafeEnginePtr(pub *mut std::ffi::c_void);
unsafe impl Send for SafeEnginePtr {}
unsafe impl Sync for SafeEnginePtr {}

/// 🚦 NeuralDispatcher (The Master Router)
/// The core router that owns hardware logic and dispatches prompts across Native IPC and HTTP.
pub struct NeuralDispatcher {
    pub booster_state: BoosterControl,
    pub current_signature: KernelSignature,
    pub cached_engine: std::sync::Arc<tokio::sync::Mutex<Option<(PathBuf, SafeEnginePtr, std::sync::Arc<libloading::Library>)>>>,
    /// 🔢 Limits concurrent LLM dispatches to prevent system overload (acts as an inference queue)
    pub inference_semaphore: Arc<tokio::sync::Semaphore>,
    /// 🛑 Per-instance cancellation flag — set to true to stop the active generation
    pub cancel_flag: Arc<AtomicBool>,
}

impl NeuralDispatcher {
    pub fn new(booster_state: BoosterControl, signature: KernelSignature) -> Self {
        Self {
            booster_state,
            current_signature: signature,
            cached_engine: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            // Max 4 concurrent LLM generations — extras wait in queue
            inference_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Primary entry point for real-time token streaming.
    /// Used by both the FFI Named Pipes (Native Desktop) and HTTP SSE (External).
    pub async fn dispatch_stream(&self, prompt: &str, skip_brain: bool) -> EngineResponse {
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

                    let active_path = resolve_active_model_path();
                    let model_path = match active_path {
                        Some(ref path) => path.clone(),
                        None => {
                            tracing::error!(
                                "❌ [Dispatcher] No active model configured. \
                                 Check ~/.cluaiz/engine/config/Permission.json \
                                 and verify the model directory exists under ~/.cluaiz/models/chat/."
                            );
                            let _ = tx.send(
                                "Error: No active model is configured. \
                                 Please set a model in Permission.json or via the /models/load API."
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
                        if let Some((_, safe_ptr, ref lib)) = engine_lock.take() {
                            unsafe {
                                if let Ok(free_fn) = lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void)>(b"cluaiz_kernel_free") {
                                    tracing::info!("🗑️ [Dispatcher] Freeing previous model instance");
                                    free_fn(safe_ptr.0);
                                }
                            }
                        }

                        // Resolve path and load DLL
                        let target_os = std::env::consts::OS;
                        let ext = match target_os {
                            "windows" => "dll",
                            "macos" => "dylib",
                            _ => "so",
                        };
                        let prefix = if target_os == "windows" { "" } else { "lib" };
                        let binary_name = format!("{}cluaiz-llama.{}", prefix, ext);
                        
                        let binary_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                            .join(&binary_name);
                            
                        // 🛡️ Strict FFI Validation Boundary
                        let marker_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
                            .join("cluaiz-llama.ready");
                            
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
                                
                                if let Ok(instantiate_fn) = library_arc.get::<unsafe extern "C" fn(*const std::os::raw::c_char, *const std::ffi::c_void) -> *mut std::ffi::c_void>(b"cluaiz_kernel_instantiate") {
                                    let c_path = std::ffi::CString::new(model_path.to_string_lossy().to_string()).unwrap();
                                    tracing::info!("🔗 [Dispatcher] Instantiating kernel with model path: {:?}", model_path);
                                    let engine_ptr = instantiate_fn(c_path.as_ptr() as *const std::os::raw::c_char, std::ptr::null());
                                    
                                    if !engine_ptr.is_null() {
                                        *engine_lock = Some((model_path.clone(), SafeEnginePtr(engine_ptr), library_arc));
                                        successfully_loaded = true;
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
                                let c_prompt = std::ffi::CString::new(prompt_clone).unwrap();

                                // 🛑 CANCELLATION-AWARE CALLBACK
                                // user_data carries (tx, cancel_flag, buffer) packed as raw ptr.
                                struct CallbackData {
                                    tx: tokio::sync::mpsc::Sender<String>,
                                    cancel_flag: Arc<AtomicBool>,
                                    buffer: std::sync::Mutex<String>,
                                }

                                extern "C" fn callback(token_ptr: *const std::os::raw::c_char, user_data: *mut std::ffi::c_void) -> bool {
                                    let data = unsafe { &*(user_data as *const CallbackData) };
                                    
                                    if data.cancel_flag.load(Ordering::Relaxed) {
                                        tracing::info!("🛑 [Dispatcher] Inference cancelled via cancel_flag.");
                                        return false;
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
                                                *buffer = buffer[buffer.len() - 100..].to_string();
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
                                };
                                let tx_ptr = &callback_data as *const CallbackData as *mut std::ffi::c_void;
                                let engine_raw = safe_ptr.0 as usize;
                                let prompt_raw = c_prompt.as_ptr() as usize;
                                let tx_raw = tx_ptr as usize;
                                let cb_raw = callback as usize;
                                let gen_raw = *gen_stream_fn as usize;

                                // 🛡️ FFI PANIC BOUNDARY & BLOCKING THREAD POOL
                                // Offload heavy FFI execution and prevent blocking the async executor.
                                let result = tokio::task::spawn_blocking(move || {
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        let callback_fn: extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool = unsafe { std::mem::transmute(cb_raw) };
                                        let gen_fn: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, usize, extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool, *mut std::ffi::c_void) = unsafe { std::mem::transmute(gen_raw) };
                                        unsafe {
                                            gen_fn(engine_raw as *mut _, prompt_raw as *const _, 4096, callback_fn, tx_raw as *mut _);
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
    pub async fn dispatch_prompt(&self, prompt: &str) -> Result<String> {
        let mut stream = match self.dispatch_stream(prompt, false).await {
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
}

/// 🚥 EmbeddingDispatcher
/// Routes embedding requests to ONNX dynamically via libloading.
pub struct EmbeddingDispatcher {
    active_lib: std::sync::Arc<libloading::Library>,
    engine_ptr: *mut std::ffi::c_void,
}

unsafe impl Send for EmbeddingDispatcher {}
unsafe impl Sync for EmbeddingDispatcher {}

               impl EmbeddingDispatcher {
    pub fn new() -> Result<Self> {
        let target_os = std::env::consts::OS;
        let ext = match target_os {
            "windows" => "dll",
            "macos" => "dylib",
            _ => "so",
        };
        let prefix = if target_os == "windows" { "" } else { "lib" };
        let binary_name = format!("{}cluaiz-onnx.{}", prefix, ext);
        
        // Use persistence or fallback to target/debug
        let binary_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
            .join(&binary_name);
            
        // 🛡️ Strict FFI Validation Boundary
        let marker_path = cluaiz_shared::HardwareGovernor::resolve_interface_path()
            .join("cluaiz-onnx.ready");
            
        if !binary_path.exists() || !marker_path.exists() {
            return Err(anyhow::anyhow!("FFI Validation Failed: ONNX kernel binary or manifest missing at {:?}", binary_path));
        }

        unsafe {
            #[cfg(windows)]
            let lib: libloading::Library = {
                // LOAD_WITH_ALTERED_SEARCH_PATH (0x00000008) forces Windows to search for dependent DLLs
                // (like onnxruntime_providers_cuda.dll) in the same directory as the kernel DLL being loaded.
                // GAP A FIX: Inject engine/drivers/ into PATH so it can find onnxruntime.dll
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
                .map_err(|_| anyhow::anyhow!("Invalid ONNX Kernel: 'cluaiz_kernel_init' missing"))?;
            init();

            let instantiate_fn: libloading::Symbol<unsafe extern "C" fn(*const std::os::raw::c_char, *const std::ffi::c_void) -> *mut std::ffi::c_void> = 
                lib.get(b"cluaiz_kernel_instantiate")
                .map_err(|_| anyhow::anyhow!("Invalid ONNX Kernel: 'cluaiz_kernel_instantiate' missing"))?;
            
            let c_path = std::ffi::CString::new("default")?;
            let engine_ptr = instantiate_fn(c_path.as_ptr() as *const std::os::raw::c_char, std::ptr::null());
            
            if engine_ptr.is_null() {
                return Err(anyhow::anyhow!("ONNX Kernel Instantiation Failed"));
            }

            tracing::info!("✅ [Dispatcher] ONNX Kernel Dynamically Linked.");
            Ok(Self {
                active_lib: std::sync::Arc::new(lib),
                engine_ptr,
            })
        }
    }

    pub fn dispatch_embedding(&self, text: &str) -> Result<Vec<f32>> {
        use neural_core::interfaces::router_contract::EmbeddingDriver;
        tracing::info!("🚥 [Dispatcher] Routing embedding request dynamically to ONNX FFI...");
        self.gen_embedding(text).map_err(|e| anyhow::anyhow!("Embedding Error: {:?}", e))
    }

    pub fn dispatch_multimodal(&self, bytes: &[u8], modality: neural_core::interfaces::router_contract::Modality) -> Result<Vec<f32>> {
        use neural_core::interfaces::router_contract::EmbeddingDriver;
        self.gen_multimodal_embedding(bytes, modality).map_err(|e| anyhow::anyhow!("Multimodal Error: {:?}", e))
    }
}

impl neural_core::interfaces::router_contract::EmbeddingDriver for EmbeddingDispatcher {
    fn gen_embedding(&self, text: &str) -> Result<Vec<f32>, neural_core::interfaces::router_contract::EngineError> {
        unsafe {
            let gen_emb_fn: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char, *mut f32, usize, *mut usize) -> i32> = 
                match self.active_lib.get(b"cluaiz_kernel_generate_embedding") {
                    Ok(f) => f,
                    Err(_) => return Err(neural_core::interfaces::router_contract::EngineError::EmbeddingFailed("Symbol missing".to_string()))
                };
            
            let c_prompt = std::ffi::CString::new(text).map_err(|_| neural_core::interfaces::router_contract::EngineError::EmbeddingFailed("CString conversion failed".to_string()))?;
            let max_dims = 8192;
            let mut out_buffer = vec![0.0f32; max_dims];
            let mut out_len: usize = 0;

            let status = gen_emb_fn(
                self.engine_ptr, 
                c_prompt.as_ptr() as *const std::os::raw::c_char, 
                out_buffer.as_mut_ptr(),
                max_dims,
                &mut out_len as *mut usize
            );
            
            if status != 0 {
                return Err(neural_core::interfaces::router_contract::EngineError::EmbeddingFailed(format!("Code: {}", status)));
            }
            
            out_buffer.truncate(out_len);
            Ok(out_buffer)
        }
    }

    fn gen_multimodal_embedding(&self, _bytes: &[u8], _modality: neural_core::interfaces::router_contract::Modality) -> Result<Vec<f32>, neural_core::interfaces::router_contract::EngineError> {
        Err(neural_core::interfaces::router_contract::EngineError::UnsupportedModality("Multimodal FFI not implemented yet".to_string()))
    }
}

impl Drop for EmbeddingDispatcher {
    fn drop(&mut self) {
        if !self.engine_ptr.is_null() {
            unsafe {
                if let Ok(free_fn) = self.active_lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void)>(b"cluaiz_kernel_free") {
                    free_fn(self.engine_ptr);
                }
            }
        }
    }
}
