use crate::ffi::llama_cpp::{self, LlamaContextParams, LlamaModelParams};
use cluaiz_shared::StructuralDNA;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{info, warn};

pub struct NativeLlama {
    pub model_ptr: *mut std::ffi::c_void,
    pub ctx_ptr: *mut std::ffi::c_void,
    pub interrupt_signal: Arc<AtomicBool>,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub kv_cache_quantization_mode: u8,
    pub context_shifting_mode: u8,
    pub speculative_decoding_mode: u8,
    pub moe_controller: Option<Arc<std::sync::Mutex<crate::expert_offloading::GgufMoeStreamingController>>>,
}

fn extract_layer_from_name(name: &str) -> Option<usize> {
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "blk" || *part == "layers" || *part == "layer") && i + 1 < parts.len() {
            if let Ok(idx) = parts[i + 1].parse::<usize>() {
                return Some(idx);
            }
        }
    }
    None
}

std::thread_local! {
    static LAST_LOGGED_LAYER: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
}

unsafe extern "C" fn ggml_sched_eval_callback(
    t: *mut std::ffi::c_void,
    ask: bool,
    user_data: *mut std::ffi::c_void,
) -> bool {
    if !ask && !user_data.is_null() {
        let name_ptr = crate::ffi::llama_cpp::ggml_get_name(t);
        if !name_ptr.is_null() {
            let name_cstr = std::ffi::CStr::from_ptr(name_ptr);
            let name_str = name_cstr.to_string_lossy();
            let name_lower = name_str.to_lowercase();
            
            // Dynamic check matching all expert tensor patterns
            let is_expert_tensor = name_lower.contains("ffn_gate_exps")
                || name_lower.contains("ffn_up_exps")
                || name_lower.contains("ffn_down_exps")
                || name_lower.contains("ffn_experts")
                || (name_lower.contains("ffn_") && name_lower.contains("exp"));

            if is_expert_tensor {
                if let Some(layer_idx) = extract_layer_from_name(&name_str) {
                    let mutex_ptr = user_data as *const std::sync::Mutex<crate::expert_offloading::GgufMoeStreamingController>;
                    if let Ok(mut controller) = (*mutex_ptr).lock() {
                        let mut should_log = false;
                        LAST_LOGGED_LAYER.with(|cell| {
                            if cell.get() != Some(layer_idx) {
                                cell.set(Some(layer_idx));
                                should_log = true;
                            }
                        });

                        if should_log {
                            let prefetch_status = if layer_idx + 1 < controller.moe_info.moe_layer_count {
                                format!("Prefetching Layer {}", layer_idx + 1)
                            } else {
                                "End of Model".to_string()
                            };
                            let discard_status = if layer_idx > 0 {
                                format!("Discarding Layer {}", layer_idx - 1)
                            } else {
                                "None".to_string()
                            };
                            eprintln!(
                                "🌊 [MoeStreaming] Active CPU Layer: {} | {} | {}",
                                layer_idx, prefetch_status, discard_status
                            );
                        }

                        // Advisory hints are issued on-demand via on_routing_decision per token.
                        // Per-tensor blanket loops across all 128 experts are disabled to prevent page cache thrashing.
                    }
                }
            }
        }
    }
    true
}

/// 🤫 Sovereign Silence: Mute verbose native logs to prevent TUI visual noise.
#[allow(dead_code)]
extern "C" fn silent_llama_log(
    _level: i32,
    _text: *const c_char,
    _user_data: *mut std::ffi::c_void,
) {
}

const NOISE_PATTERNS: &[&str] = &[
    "load_tensors:",
    "create_tensor:",
    "CUDA Graph",
    "ggml_backend_cuda_graph_compute",
    "llama_kv_cache",
    ".",
];

extern "C" fn llama_log_callback(
    _level: std::ffi::c_int,
    text: *const std::ffi::c_char,
    _user_data: *mut std::ffi::c_void,
) {
    unsafe {
        if text.is_null() { return; }
        let c_str = std::ffi::CStr::from_ptr(text);
        let s = c_str.to_string_lossy();
        let trimmed = s.trim();
        if !trimmed.is_empty() && trimmed != "." && !NOISE_PATTERNS.iter().any(|&p| trimmed.contains(p)) {
            eprintln!("📢 [llama.cpp] {}", trimmed);
        }
    }
}

pub fn print_memory_trace(step: &str) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let free_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    let (free_vram_bytes, total_vram_bytes) = crate::cuda_dma_streamer::CudaDmaStreamer::get_live_vram_info();
    let free_vram_gb = free_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_vram_gb = total_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    eprintln!(
        "🆘 [MEMORY TRACE: {}] Free RAM: {:.2} GB / {:.2} GB | Free VRAM: {:.2} GB / {:.2} GB",
        step, free_ram_gb, total_ram_gb, free_vram_gb, total_vram_gb
    );
}

impl NativeLlama {
    /// 🧬 Load a model and initialize context with industrial booster params.
    pub fn load(
        model_path: &str,
        model_params: LlamaModelParams,
        mut ctx_params: LlamaContextParams,
        dna: &mut cluaiz_shared::metadata::dna::StructuralDNA,
        kv_cache_quantization_mode: u8,
        context_shifting_mode: u8,
        speculative_decoding_mode: u8,
        moe_controller: Option<Arc<std::sync::Mutex<crate::expert_offloading::GgufMoeStreamingController>>>,
    ) -> anyhow::Result<Self> {
        // 🛡️ INTERCEPT INTERNAL LOGS TO SEE FATAL ERRORS
        unsafe {
            llama_cpp::llama_log_set(Some(llama_log_callback), std::ptr::null_mut());
        }

        // ══ SOVEREIGN OPTIMIZATION (Hardware Overrides) ══
        // We now use llama_log_callback to pipe all logs instead of swallowing them.

        // 🚀 Backend Init: Already handled globally by cluaiz_kernel_init() in ffi_exports.rs.
        // DO NOT call llama_backend_init() here — it WIPES existing CUDA registration.
        // DO NOT register CUDA here — it causes VRAM allocation even in CPU mode (n_gpu_layers=0)
        // because #[cfg(feature = "cuda")] is compile-time, not runtime.

        let c_path = CString::new(model_path)?;

        print_memory_trace("1. BEFORE MODEL LOAD");
        eprintln!("📊 [Native-Llama] FFI Parameters: n_gpu_layers = {}, use_mmap = {}, use_mlock = {}, n_threads = {}, n_threads_batch = {}", model_params.n_gpu_layers, model_params.use_mmap, model_params.use_mlock, ctx_params.n_threads, ctx_params.n_threads_batch);
        info!(
            "🧬 [Native-Llama] Loading model: {} | ctx: {} tokens",
            model_path, ctx_params.n_ctx
        );
        let mut model_ptr =
            unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), model_params) };

        print_memory_trace("2. AFTER MODEL LOAD");

        // 🔒 Mlock Graceful Fallback
        if model_ptr.is_null() && model_params.use_mlock {
            warn!("🔒 [Arbiter] mlock failed. Falling back to high-speed mmap...");
            let mut fallback_params = model_params;
            fallback_params.use_mlock = false;
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), fallback_params) };
        }

        // 🛡️ CERD DOCTRINE GPU-FALLBACK (No Hardcoded Strings)
        // If Model Load fails with n_gpu_layers != 0 (e.g. -1 for all layers, or >0), the CUDA backend 
        // likely doesn't support the tensor format (e.g., TQ1_0 or TQ2_0 BitNet models). 
        // We must gracefully fallback to CPU-only.
        if model_ptr.is_null() && model_params.n_gpu_layers > 1 {
            cluaiz_shared::dev_info!("⚠️ [Native-Llama] VRAM allocation pressure during model load. Clamping n_gpu_layers to {} and retrying...", model_params.n_gpu_layers / 2);
            let mut half_params = model_params;
            half_params.n_gpu_layers /= 2;
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), half_params) };
        }

        // 🛡️ CERD DOCTRINE GPU-FALLBACK (No Hardcoded Strings)
        // If Model Load fails with n_gpu_layers != 0 (e.g. -1 for all layers, or >0), the CUDA backend 
        // likely doesn't support the tensor format (e.g., TQ1_0 or TQ2_0 BitNet models). 
        // We must gracefully fallback to CPU-only.
        if model_ptr.is_null() && model_params.n_gpu_layers != 0 {
            eprintln!("⚠️ [Native-Llama] Model Load Failed on GPU. Falling back to CPU-only...");
            let mut cpu_params = model_params;
            cpu_params.n_gpu_layers = 0; // Force CPU
            cpu_params.no_host = false;  // CRITICAL: Must allow host memory allocation for CPU inference!
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), cpu_params) };
        }

        // 🚨 Ultimate Mmap Fallback
        // If it STILL fails on CPU, it might be an mmap mapping limitation (e.g. Windows file locking or unsupported tensor alignment).
        if model_ptr.is_null() && model_params.use_mmap {
            // Check available system RAM first to prevent disk thrashing OOM
            let model_size_bytes = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
            let model_size_gb = model_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            let available_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

            if model_size_gb > available_ram_gb {
                eprintln!(
                    "❌ [Native-Llama] Model size ({:.2} GB) exceeds available system RAM ({:.2} GB). \
                     Refusing to fall back to use_mmap = false to prevent SSD pagefile thrashing and lockup.",
                    model_size_gb,
                    available_ram_gb
                );
                return Err(anyhow::anyhow!(
                    "Insufficient RAM to load model. Model: {:.2} GB, Available RAM: {:.2} GB. SSD thrashing prevented.",
                    model_size_gb,
                    available_ram_gb
                ));
            }

            eprintln!("⚠️ [Native-Llama] Model Load Failed with mmap. Falling back to RAM allocation (use_mmap = false)...");
            let mut ram_params = model_params;
            ram_params.n_gpu_layers = 0;
            ram_params.no_host = false;
            ram_params.use_mmap = false;
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), ram_params) };
        }

        if model_ptr.is_null() {
            return Err(anyhow::anyhow!("Model Load Failure: {}", model_path));
        }

        // 🌊 POST-LOAD MEMORY PURGE: Release cold expert pages from physical RAM immediately
        if let Some(ref controller) = moe_controller {
            if let Ok(ctrl) = controller.lock() {
                ctrl.purge_all_experts_except(0);
            }
        }

        let model_dir = std::path::Path::new(model_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        cluaiz_shared::dev_info!(
            "🧬 [Native-Llama] Starting DNA Discovery for: {:?}",
            model_dir
        );

        dna.weights_already_loaded = true;

        // Save the requested context size to prevent it from being overwritten by the global GPU VRAM arbiter in CPU-only mode
        let requested_n_ctx = ctx_params.n_ctx;

        if let Err(e) = dna.discover_from_path(model_dir) {
            cluaiz_shared::dev_info!("⚠️ [Native-Llama] DNA Discovery Failed: {}", e);
        }

        // Ensure n_ctx is strictly bound by Negotiator's requested limit, preventing 256k token (14.4 GB) KV Cache allocations
        ctx_params.n_ctx = requested_n_ctx;
        info!(
            "🎯 [Native-Llama] SOVEREIGN HANDSHAKE: Context Window strictly locked to: {} tokens",
            ctx_params.n_ctx
        );

        let mut speculative_decoding_mode = speculative_decoding_mode;
        if dna.model_identity.to_lowercase().contains("gemma") {
            info!("🛡️ [Native-Llama] Gemma model detected: Disabling speculative decoding.");
            speculative_decoding_mode = 0;
        }

        unsafe {
            // 🚀 Force GGML CUDA to offload RAM tensor operations to GPU even for single-token stream decoding
            if model_params.n_gpu_layers != 0 {
                std::env::set_var("GGML_OP_OFFLOAD_MIN_BATCH", "1");
                std::env::set_var("GGML_CUDA_FORCE_MMQ", "1");
            }

            let current_graphs = std::env::var("GGML_CUDA_USE_GRAPHS").unwrap_or_default();
            let target_graphs = if speculative_decoding_mode == 1 || speculative_decoding_mode == 2 {
                "0"
            } else {
                "1"
            };
            if current_graphs != target_graphs {
                std::env::set_var("GGML_CUDA_USE_GRAPHS", target_graphs);
            }
        }

        // 🚀 Ensure dynamic host-tensor op offloading to CUDA device
        if model_params.n_gpu_layers != 0 {
            ctx_params.op_offload = 1;
            ctx_params.offload_kqv = 1;
        }

        // 🛡️ Disable C-FFI per-tensor eval callback to eliminate CUDA Pointer Desynchronization
        // and OS interrupt thrashing. Model stays fully resident in statically assigned VRAM/RAM.
        ctx_params.cb_eval = std::ptr::null_mut();
        ctx_params.cb_eval_user_data = std::ptr::null_mut();

        print_memory_trace("3. BEFORE CONTEXT CREATION");
        let mut ctx_ptr = unsafe { llama_cpp::llama_init_from_model(model_ptr, ctx_params) };
        print_memory_trace("4. AFTER CONTEXT CREATION");

        // 🛡️ CERD DOCTRINE FA-FALLBACK (No Hardcoded Strings)
        // If Context Init fails, gracefully retry without Flash Attention while PRESERVING quantized KV cache (no F16 explosion)
        if ctx_ptr.is_null() && ctx_params.flash_attn_type > 0 {
            cluaiz_shared::dev_info!("⚠️ [Native-Llama] Context Init Failed with Flash Attention ON. Initiating Safe Fallback (keeping quantized KV cache)...");
            let mut fallback_ctx_params = ctx_params;
            fallback_ctx_params.flash_attn_type = 0;
            // Preserve original type_k and type_v so RAM doesn't blow up to 47GB!
            ctx_ptr = unsafe { llama_cpp::llama_init_from_model(model_ptr, fallback_ctx_params) };
        }

        if ctx_ptr.is_null() {
            unsafe { llama_cpp::llama_model_free(model_ptr) };
            return Err(anyhow::anyhow!("Context Init Failure"));
        }

        Ok(Self {
            model_ptr,
            ctx_ptr,
            interrupt_signal: Arc::new(AtomicBool::new(false)),
            n_ctx: ctx_params.n_ctx,
            n_batch: if ctx_params.n_ctx == 0 { ctx_params.n_batch } else { std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch) },
            kv_cache_quantization_mode,
            context_shifting_mode,
            speculative_decoding_mode,
            moe_controller,
        })
    }

    pub fn resize_context(&mut self, ctx_params: LlamaContextParams) -> anyhow::Result<()> {
        if self.model_ptr.is_null() {
            return Err(anyhow::anyhow!("Cannot resize context: Model not loaded"));
        }
        unsafe {
            if !self.ctx_ptr.is_null() {
                llama_cpp::llama_free(self.ctx_ptr);
            }
            self.ctx_ptr = llama_cpp::llama_init_from_model(self.model_ptr, ctx_params);
            if self.ctx_ptr.is_null() {
                return Err(anyhow::anyhow!("Context Resize Failure"));
            }
            self.n_ctx = ctx_params.n_ctx;
            self.n_batch = if ctx_params.n_ctx == 0 { ctx_params.n_batch } else { std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch) };
        }
        Ok(())
    }

    pub fn stitch_signal(&mut self, signal_id: i32, offset: i32, length: i32) -> anyhow::Result<()> {
        unsafe {
            let memory = llama_cpp::llama_get_memory(self.ctx_ptr);
            llama_cpp::llama_memory_seq_cp(memory, signal_id, 0, 0, length);
        }
        Ok(())
    }

    /// 💾 Save Prompt Cache to disk
    pub fn save_prompt_cache(&self, path: &str, tokens: &[i32]) -> anyhow::Result<()> {
        info!("💾 [Native-Llama] Saving prompt cache to: {}", path);
        let c_path = std::ffi::CString::new(path)?;
        unsafe {
            let success = llama_cpp::llama_state_save_file(
                self.ctx_ptr,
                c_path.as_ptr(),
                tokens.as_ptr(),
                tokens.len(),
            );
            if !success {
                return Err(anyhow::anyhow!("Failed to save prompt cache to {}", path));
            }
        }
        Ok(())
    }

    /// 💾 Load Prompt Cache from disk
    pub fn load_prompt_cache(&mut self, path: &str) -> anyhow::Result<Vec<i32>> {
        info!("💾 [Native-Llama] Loading prompt cache from: {}", path);
        let c_path = std::ffi::CString::new(path)?;
        let mut tokens = vec![0i32; self.n_ctx as usize];
        let mut n_tokens_out: usize = 0;

        unsafe {
            let success = llama_cpp::llama_state_load_file(
                self.ctx_ptr,
                c_path.as_ptr(),
                tokens.as_mut_ptr(),
                tokens.len(),
                &mut n_tokens_out as *mut usize,
            );
            if !success {
                return Err(anyhow::anyhow!("Failed to load prompt cache from {}", path));
            }
        }
        tokens.truncate(n_tokens_out);
        Ok(tokens)
    }

    /// 🧠 Prefill a prompt into the KV cache (Context State) without generating tokens.
    pub fn prefill_prompt(&mut self, prompt: &str) -> anyhow::Result<Vec<i32>> {
        unsafe {
            // 🧹 Sovereign Flush: Ensure KV cache is clear before starting new prefill
            let mem = llama_cpp::llama_get_memory(self.ctx_ptr);
            llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);

            let vocab = llama_cpp::llama_model_get_vocab(self.model_ptr);
            let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

            if n_vocab <= 0 {
                return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
            }

            let c_prompt = std::ffi::CString::new(prompt.to_string())?;

            // 1. Tokenize
            cluaiz_shared::dev_info!(
                "🧠 [Native-Llama] Starting tokenization of prompt (len: {})...",
                prompt.len()
            );
            let mut tokens = vec![0i32; prompt.len() + 8];
            let n_tokens = llama_cpp::llama_tokenize(
                vocab,
                c_prompt.as_ptr(),
                prompt.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                true,
                true,
            );

            if n_tokens < 0 {
                println!("❌ [Native-Llama] Tokenization failed!");
                return Err(anyhow::anyhow!("Tokenization failed"));
            }
            tokens.truncate(n_tokens as usize);
            println!(
                "🧠 [Native-Llama] Tokenization successful: {} tokens",
                tokens.len()
            );

            // 2. Initial Batch Decode (Prefill) with Chunking
            let chunk_size = self.n_batch as i32;
            println!(
                "🧠 [Native-Llama] Initializing llama batch of size {}...",
                chunk_size
            );
            let mut batch = llama_cpp::llama_batch_init(chunk_size, 0, 1);

            let mut tokens_processed = 0;

            while tokens_processed < tokens.len() {
                let current_chunk =
                    std::cmp::min(chunk_size as usize, tokens.len() - tokens_processed);

                for i in 0..current_chunk {
                    let global_i = tokens_processed + i;
                    *batch.token.add(i) = tokens[global_i];
                    *batch.pos.add(i) = global_i as i32;
                    *batch.n_seq_id.add(i) = 1;
                    *(*batch.seq_id.add(i)).add(0) = 0;
                    *batch.logits.add(i) = if global_i == tokens.len() - 1 { 1 } else { 0 };
                }
                batch.n_tokens = current_chunk as i32;

                println!(
                    "🧠 [Native-Llama] Prefilling chunk of {} tokens ({} / {})...",
                    current_chunk,
                    tokens_processed + current_chunk,
                    tokens.len()
                );
                if llama_cpp::llama_decode(self.ctx_ptr, batch) != 0 {
                    println!("❌ [Native-Llama] llama_decode failed!");
                    llama_cpp::llama_batch_free(batch);
                    return Err(anyhow::anyhow!(
                        "Prefill decode failed at chunk starting at {}",
                        tokens_processed
                    ));
                }
                println!("🧠 [Native-Llama] Decoded chunk successfully");

                tokens_processed += current_chunk;
            }

            println!("🧠 [Native-Llama] Prefill complete, freeing batch...");
            llama_cpp::llama_batch_free(batch);

            Ok(tokens)
        }
    }

    pub fn stream_tokens(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        dna: &StructuralDNA,
        last_prefilled_tokens: &[i32],
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<Vec<i32>> {
        crate::native::stream::stream_tokens(
            self,
            prompt,
            max_tokens,
            dna,
            last_prefilled_tokens,
            callback,
        )
    }
}

impl Drop for NativeLlama {
    fn drop(&mut self) {
        unsafe {
            if !self.ctx_ptr.is_null() {
                llama_cpp::llama_free(self.ctx_ptr);
            }
            if !self.model_ptr.is_null() {
                llama_cpp::llama_model_free(self.model_ptr);
            }
        }
    }
}

unsafe impl Send for NativeLlama {}
unsafe impl Sync for NativeLlama {}
