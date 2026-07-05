use super::*;
// ─── Sovereign FFI Gateway ──────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn cluaiz_kernel_init() -> *const std::os::raw::c_char {
    unsafe {
        // 🤫 Sovereign Silence: Hard-redirect native stdout/stderr to NUL
        // This stops all non-callback logs (CUDA Graph, etc.) from polluting the TUI.
        /* 🧪 Debug Mode: Temporarily disabled NUL redirection
        #[cfg(windows)]
        {
            let n_path = std::ffi::CString::new("NUL").unwrap();
            let mode = std::ffi::CString::new("w").unwrap();
            libc::freopen(n_path.as_ptr(), mode.as_ptr(), libc::stdout);
            libc::freopen(n_path.as_ptr(), mode.as_ptr(), libc::stderr);
        }
        */
        #[cfg(not(windows))]
        {
            let n_path = std::ffi::CString::new("/dev/null").unwrap();
            let mode = std::ffi::CString::new("w").unwrap();
            libc::freopen(n_path.as_ptr(), mode.as_ptr(), libc::stdout);
            libc::freopen(n_path.as_ptr(), mode.as_ptr(), libc::stderr);
        }

        // Also set the callback for handled logs
        extern "C" fn verbose_log(
            _level: i32,
            text: *const std::os::raw::c_char,
            _data: *mut std::ffi::c_void,
        ) {
            let s = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy();
            eprint!("{}", s);
        }
        crate::ffi::llama_cpp::llama_log_set(Some(verbose_log), std::ptr::null_mut());

        ffi::llama_cpp::llama_backend_init();

        #[cfg(feature = "cuda")]
        {
            let reg = ffi::llama_cpp::ggml_backend_cuda_reg();
            if !reg.is_null() {
                ffi::llama_cpp::ggml_backend_register(reg);
                tracing::info!("🟢 [Llama-Engine] CUDA Backend explicitly re-registered after init.");
            }
        }
    }
    tracing::info!("🧬 [Llama.cpp-Kernel] Sovereign Handshake & Backend Initialized.");
    "cluaiz-llama.cpp-active\0".as_ptr() as *const std::os::raw::c_char
}

#[used]
static _FORCE_KEEP_INIT: extern "C" fn() -> *const std::os::raw::c_char = cluaiz_kernel_init;

#[no_mangle]
pub extern "C" fn cluaiz_kernel_instantiate(
    path_ptr: *const std::os::raw::c_char,
    booster_ptr: *const cluaiz_shared::hardware::schema::booster::cluaizBoosterContext,
) -> *mut RuntimeB {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let path_str = unsafe { std::ffi::CStr::from_ptr(path_ptr) }
            .to_string_lossy()
            .into_owned();

        let model_path = std::path::Path::new(&path_str);
        let model_dir = model_path.parent().unwrap_or(model_path);

        cluaiz_shared::dev_info!(
            "🧬 [Llama-Lib] Initiating Sovereign DNA Handshake for: {:?}",
            model_dir
        );
        let mut dna = cluaiz_shared::metadata::dna::StructuralDNA::load(
            &model_dir.join("structural_dna.json"),
        )
        .unwrap_or_else(|_| {
            cluaiz_shared::dev_info!("⚠️ [Llama-Lib] DNA Manifest missing. Creating transient skeleton...");
            cluaiz_shared::metadata::dna::StructuralDNA::default()
        });

        // ALWAYS perform real-time discovery to sync with LIVE hardware state
        cluaiz_shared::dev_info!("📂 [Llama-Lib] Discovering real-time truth...");
        if let Err(e) = dna.discover_from_path(model_dir) {
            cluaiz_shared::dev_info!(
                "⚠️ [Llama-Lib] DNA Discovery Failed: {}. Using best-effort constraints.",
                e
            );
        }
        cluaiz_shared::dev_info!(
            "✅ [Llama-Lib] DNA Discovery Complete. Negotiated Context: {:?}",
            dna.max_context_length
        );
        cluaiz_shared::dev_info!("📊 [Llama-Lib] Weights Size: {:.2}GB", dna.weights_size_gb);

        let context = cluaizContext::boot(dna, cluaiz_shared::TemplateManager::default());
        let mut engine = Box::new(RuntimeB::new(&path_str, context));

        // Inject Booster Configuration from Caller
        if !booster_ptr.is_null() {
            let booster_ctx = unsafe { *booster_ptr };
            cluaiz_shared::dev_info!(
                "🚀 [Llama.cpp-Kernel] Received cluaizBoosterContext via FFI: {:?}",
                booster_ctx
            );
            tracing::info!(
                "🚀 [Llama.cpp-Kernel] Received cluaizBoosterContext via FFI: {:?}",
                booster_ctx
            );
            engine.booster.flash_attn = booster_ctx.flash_attention;
            engine.booster.n_gpu_layers = booster_ctx.n_gpu_layers;
            engine.booster.turbo_quant = if booster_ctx.turbo_quant {
                "active".to_string()
            } else {
                "none".to_string()
            };
            engine.booster.kv_cache_quantization = match booster_ctx.kv_cache_quantization_mode {
                1 => "Kv8".to_string(),
                2 => "Kv4".to_string(),
                _ => "Auto".to_string(),
            };
            engine.booster.context_shifting = match booster_ctx.context_shifting_mode {
                0 => "Off".to_string(),
                1 => "Minimal".to_string(),
                2 => "Standard".to_string(),
                3 => "Aggressive".to_string(),
                4 => "Extreme".to_string(),
                _ => "Auto".to_string(),
            };
            engine.booster.speculative_decoding = match booster_ctx.speculative_decoding_mode {
                0 => "Off".to_string(),
                1 => "On".to_string(),
                2 => "Auto".to_string(),
                _ => "Auto".to_string(),
            };
            engine.booster.use_mmap = true;

            if booster_ctx.max_context_length > 0 {
                engine.context.dna.max_context_length =
                    Some(booster_ctx.max_context_length as usize);
            }
        } else {
            // Self-load from Binary Booster Truth if FFI was blank
            if let Ok(booster) =
                cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings()
            {
                let _ = engine.apply_booster(&booster);
            }
        }

        // 🧬 Trigger Native Load immediately on instantiation
        if let Err(e) = engine.load_native() {
            cluaiz_shared::dev_info!("❌ [Llama.cpp-Kernel] Native Load Failed: {}", e);
            tracing::error!("❌ [Llama.cpp-Kernel] Native Load Failed: {}", e);
            return std::ptr::null_mut();
        }

        Box::into_raw(engine)
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            tracing::error!(
                "🚨 [FFI-Panic] Caught panic in cluaiz_kernel_instantiate! Preventing OS crash."
            );
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn cluaiz_kernel_generate_stream(
    engine_ptr: *mut RuntimeB,
    prompt_ptr: *const std::os::raw::c_char,
    max_tokens: usize,
    callback: extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool,
    user_data: *mut std::ffi::c_void,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine_ptr.is_null() {
            return -1;
        }
        let engine = unsafe { &mut *engine_ptr };

        let prompt = unsafe { std::ffi::CStr::from_ptr(prompt_ptr) }
            .to_string_lossy()
            .into_owned();

        let user_data_ptr = user_data as usize;
        let callback_ptr = callback as usize;

        let rust_callback = Box::new(move |token: String| -> bool {
            let c_str = std::ffi::CString::new(token).unwrap_or_default();
            let cb = unsafe {
                std::mem::transmute::<
                    usize,
                    extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool,
                >(callback_ptr)
            };
            let ud = user_data_ptr as *mut std::ffi::c_void;
            unsafe { (cb)(c_str.as_ptr(), ud) }
        });

        match engine.generate_stream(&prompt, max_tokens, rust_callback) {
            Ok(_) => 0,
            Err(e) => {
                cluaiz_shared::dev_info!("❌ [Llama-Engine] Generation failed: {}", e);
                tracing::error!("❌ [Llama-Engine] Generation failed: {}", e);
                -2
            }
        }
    }));

    match result {
        Ok(res) => res,
        Err(_) => {
            tracing::error!("🚨 [FFI-Panic] Caught panic in cluaiz_kernel_generate_stream!");
            -3
        }
    }
}

#[no_mangle]
pub extern "C" fn cluaiz_kernel_free(engine_ptr: *mut RuntimeB) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !engine_ptr.is_null() {
            unsafe {
                let _ = Box::from_raw(engine_ptr);
                // 🛑 CRITICAL FIX: DO NOT call llama_backend_free() here!
                // llama_backend_free() destroys the global llama.cpp state.
                // If a background thread (CompilerDaemon) instantiates and drops an engine,
                // calling this will kill the active Chat Engine in the main thread!
            }
        }
    }));
    if result.is_err() {
        tracing::error!("🚨 [FFI-Panic] Caught panic in cluaiz_kernel_free!");
    }
}

#[no_mangle]
pub extern "C" fn cluaiz_kernel_set_skip_ptr(ptr: *const std::sync::atomic::AtomicBool) {
    unsafe {
        crate::native::stream::SKIP_PTR = ptr;
    }
}

#[no_mangle]
pub extern "C" fn cluaiz_kernel_dump_kv_cache(
    engine_ptr: *mut RuntimeB,
    path_ptr: *const std::os::raw::c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine_ptr.is_null() || path_ptr.is_null() {
            return -1;
        }

        let path = unsafe { std::ffi::CStr::from_ptr(path_ptr) }
            .to_string_lossy()
            .into_owned();

        let engine = unsafe { &mut *engine_ptr };
        if let Some(ref native) = engine.native {
            // Using the FFI bindings to save KV cache state
            if !native.ctx_ptr.is_null() {
                let c_path = std::ffi::CString::new(path).unwrap_or_default();
                let bytes_written = unsafe {
                    if !engine.last_prefilled_tokens.is_empty() {
                        crate::ffi::llama_cpp::llama_state_seq_save_file(
                            native.ctx_ptr,
                            c_path.as_ptr(),
                            0, // seq_id
                            engine.last_prefilled_tokens.as_ptr(),
                            engine.last_prefilled_tokens.len(),
                        )
                    } else {
                        crate::ffi::llama_cpp::llama_state_seq_save_file(
                            native.ctx_ptr,
                            c_path.as_ptr(),
                            0, // seq_id
                            std::ptr::null(),
                            0,
                        )
                    }
                };
                if bytes_written > 0 {
                    0
                } else {
                    -2
                }
            } else {
                -3
            }
        } else {
            -4
        }
    }));

    match result {
        Ok(res) => res,
        Err(_) => -5,
    }
}

#[no_mangle]
pub extern "C" fn cluaiz_kernel_load_kv_cache(
    engine_ptr: *mut RuntimeB,
    path_ptr: *const std::os::raw::c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine_ptr.is_null() || path_ptr.is_null() {
            return -1;
        }

        let path = unsafe { std::ffi::CStr::from_ptr(path_ptr) }
            .to_string_lossy()
            .into_owned();

        let engine = unsafe { &mut *engine_ptr };
        if let Some(ref native) = engine.native {
            if !native.ctx_ptr.is_null() {
                let c_path = std::ffi::CString::new(path).unwrap_or_default();
                let mut tokens = vec![0i32; native.n_ctx as usize]; // Dynamic tokens vector
                let mut n_tokens_out: usize = 0;
                let bytes_read = unsafe {
                    crate::ffi::llama_cpp::llama_state_seq_load_file(
                        native.ctx_ptr,
                        c_path.as_ptr(),
                        0, // seq_id
                        tokens.as_mut_ptr(),
                        tokens.len(),
                        &mut n_tokens_out as *mut usize,
                    )
                };
                if bytes_read > 0 {
                    engine.last_prefilled_tokens = tokens[..n_tokens_out].to_vec();
                    0
                } else {
                    -2
                }
            } else {
                -3
            }
        } else {
            -4
        }
    }));

    match result {
        Ok(res) => res,
        Err(_) => -5,
    }
}
