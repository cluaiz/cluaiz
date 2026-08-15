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
        // 🚀 Set default op offload threshold to 1 for dynamic GPU streaming during single-token generation
        std::env::set_var("GGML_OP_OFFLOAD_MIN_BATCH", "1");

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
    optimization_ptr: *const cluaiz_shared::hardware::schema::optimization::cluaizOptimizationContext,
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

        // Inject Optimization Configuration from Caller
        if !optimization_ptr.is_null() {
            let optimization_ctx = unsafe { *optimization_ptr };
            cluaiz_shared::dev_info!(
                "🚀 [Llama.cpp-Kernel] Received cluaizOptimizationContext via FFI: {:?}",
                optimization_ctx
            );
            tracing::info!(
                "🚀 [Llama.cpp-Kernel] Received cluaizOptimizationContext via FFI: {:?}",
                optimization_ctx
            );
            engine.optimization.flash_attn = optimization_ctx.flash_attention;
            engine.optimization.n_gpu_layers = optimization_ctx.n_gpu_layers;
            engine.optimization.turbo_quant = if optimization_ctx.turbo_quant {
                "active".to_string()
            } else {
                "none".to_string()
            };
            engine.optimization.kv_cache_quantization = match optimization_ctx.kv_cache_quantization_mode {
                1 => "Kv8".to_string(),
                2 => "Kv4".to_string(),
                _ => "Auto".to_string(),
            };
            engine.optimization.context_shifting = match optimization_ctx.context_shifting_mode {
                0 => "Off".to_string(),
                1 => "Minimal".to_string(),
                2 => "Standard".to_string(),
                3 => "Aggressive".to_string(),
                4 => "Extreme".to_string(),
                _ => "Auto".to_string(),
            };
            engine.optimization.speculative_decoding = match optimization_ctx.speculative_decoding_mode {
                0 => "Off".to_string(),
                1 => "On".to_string(),
                2 => "Auto".to_string(),
                _ => "Auto".to_string(),
            };
            engine.optimization.use_mmap = true;

            if optimization_ctx.max_context_length > 0 {
                engine.context.dna.max_context_length =
                    Some(optimization_ctx.max_context_length as usize);
                engine.optimization.n_ctx = optimization_ctx.max_context_length;
            }
        } else {
            // Self-load from Binary Booster Truth if FFI was blank
            if let Ok(booster) =
                cluaiz_shared::hardware::governor::HardwareGovernor::load_optimization_settings()
            {
                let _ = engine.apply_optimization(&booster);
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

// ─── Sovereign Native GGUF Embedding FFI ──────────────────────────────────────
#[no_mangle]
pub extern "C" fn cluaiz_kernel_generate_embedding(
    engine_ptr: *mut std::ffi::c_void,
    text_ptr: *const std::os::raw::c_char,
    out_buffer: *mut f32,
    max_dims: usize,
    out_len: *mut usize,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine_ptr.is_null() || text_ptr.is_null() || out_buffer.is_null() || out_len.is_null() {
            return -1;
        }

        let engine = unsafe { &mut *(engine_ptr as *mut RuntimeB) };
        let text = unsafe { std::ffi::CStr::from_ptr(text_ptr) }.to_string_lossy();

        if let Some(ref native) = engine.native {
            if native.model_ptr.is_null() || native.ctx_ptr.is_null() {
                return -2;
            }

            unsafe {
                let vocab = ffi::llama_cpp::llama_model_get_vocab(native.model_ptr);
                let n_embd = ffi::llama_cpp::llama_model_n_embd(native.model_ptr);
                if n_embd <= 0 {
                    return -3;
                }

                let max_tokens = text.len() + 16;
                let mut tokens = vec![0i32; max_tokens];
                let c_text = match std::ffi::CString::new(text.as_bytes()) {
                    Ok(c) => c,
                    Err(_) => return -4,
                };

                let n_tokens = ffi::llama_cpp::llama_tokenize(
                    vocab,
                    c_text.as_ptr(),
                    c_text.to_bytes().len() as i32,
                    tokens.as_mut_ptr(),
                    max_tokens as i32,
                    true,  // add_special
                    false, // parse_special
                );

                if n_tokens <= 0 {
                    return -5;
                }

                // Prepare batch for embedding decode
                let mut batch = ffi::llama_cpp::llama_batch_init(n_tokens, 0, 1);
                batch.n_tokens = n_tokens;
                for i in 0..n_tokens as usize {
                    *batch.token.add(i) = tokens[i];
                    *batch.pos.add(i) = i as i32;
                    *batch.n_seq_id.add(i) = 1;
                    *(*batch.seq_id.add(i)).add(0) = 0;
                    *batch.logits.add(i) = if i == (n_tokens as usize - 1) { 1 } else { 0 };
                }

                let status = ffi::llama_cpp::llama_decode(native.ctx_ptr, batch);
                ffi::llama_cpp::llama_batch_free(batch);

                if status != 0 {
                    return -6;
                }

                let embd_ptr = ffi::llama_cpp::llama_get_embeddings(native.ctx_ptr);
                if embd_ptr.is_null() {
                    return -7;
                }

                let dims = (n_embd as usize).min(max_dims);

                // Compute L2 norm for normalization
                let mut sum_sq = 0.0f32;
                for i in 0..dims {
                    let val = *embd_ptr.add(i);
                    sum_sq += val * val;
                }
                let norm = if sum_sq > 0.0 { sum_sq.sqrt() } else { 1.0 };

                for i in 0..dims {
                    *out_buffer.add(i) = *embd_ptr.add(i) / norm;
                }
                *out_len = dims;

                0
            }
        } else {
            -8
        }
    }));

    result.unwrap_or(-9)
}
