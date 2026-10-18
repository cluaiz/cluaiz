use crate::ffi::llama_cpp::{self, LlamaContextParams, LlamaModelParams};
use cluaiz_models::types::StructuralDNA;
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
    pub moe_controller: Option<Arc<std::sync::Mutex<()>>>,
    /// n_gpu_layers the model really loaded with (differs from the request when a load fallback ran)
    pub n_gpu_layers_loaded: i32,
    /// Chat templates parsed once at load (null if llama.cpp could not parse them); freed before the model
    chat_templates: *mut std::ffi::c_void,
    /// The custom template the parsed handle was built from (`dna.chat_template` at load)
    chat_templates_src: Option<String>,
    /// Thinking budget and use of the last generation (None when the model did not think)
    pub last_thinking: Option<ThinkingReport>,
    /// What the context rules did in the last generation
    pub last_context: Option<ContextReport>,
    /// Sliding-window checkpoints of the current KV (llama-server context checkpoints), oldest first
    pub checkpoints: Vec<crate::native::context::Checkpoint>,
}

/// The request does not fit the context (llama-server's `exceed_context_size_error`). It is raised
/// before the KV cache is touched, so the cache stays valid for the next request.
#[derive(Debug)]
pub struct ContextExceeded(pub String);

impl std::fmt::Display for ContextExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ContextExceeded {}

/// What the context rules did in one generation (sent to the client in the engine report).
/// Token counts are the model's own; names follow llama-server where it has one.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ContextReport {
    pub n_ctx: usize,
    /// Prompt tokens after fitting, cached or not
    pub prompt_tokens: usize,
    /// Prompt tokens reused from the KV cache instead of being processed again
    pub cache_n: usize,
    /// Oldest messages dropped before the request so it fits, and the tokens they held
    pub dropped_messages: usize,
    pub dropped_tokens: usize,
    /// Tokens removed by context shifts while answering
    pub shifted_tokens: usize,
    /// Tokens in the context when generation ended (prompt + answer)
    pub n_tokens: usize,
    /// The context limit cut the history or the answer (llama-server `truncated`)
    pub truncated: bool,
    /// "eos" = the model ended, "limit" = max_tokens or the context ended it, "cancel", "error"
    pub stop: &'static str,
}

impl ContextReport {
    /// OpenAI `finish_reason`
    pub fn finish_reason(&self) -> &'static str {
        if self.stop == "limit" {
            "length"
        } else {
            "stop"
        }
    }
}

/// What the thinking budget did in one generation (sent to the client in the engine report).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThinkingReport {
    /// Effort asked for (auto, low, ..., or a number)
    pub effort: cluaiz_core::ThinkingBudget,
    /// Tokens thinking was allowed after the room limit
    pub budget_tokens: usize,
    /// The asked budget did not fit the context and was lowered so the answer fits
    pub capped: bool,
    /// Thinking tokens the model used
    pub used_tokens: usize,
    /// "model" = the model ended it, "budget" / "skip" = the engine closed it
    pub ended_by: &'static str,
}

/// Timings of the last generation, measured by llama.cpp itself (the numbers llama-cli and
/// llama-server print). Field names match llama-server's `timings` object.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct GenerationTimings {
    /// Prompt tokens processed (tokens reused from the KV cache are not counted)
    pub prompt_n: i32,
    pub prompt_ms: f64,
    pub prompt_per_second: f64,
    /// Tokens generated one by one
    pub predicted_n: i32,
    pub predicted_ms: f64,
    pub predicted_per_second: f64,
}

/// Tokens per llama_decode call so a prompt of `n_tokens` is split into equal chunks of at most `max`.
/// llama.cpp runs a micro-batch smaller than its op-offload size (a backend setting) on the CPU for
/// layers kept in RAM, so a 512 + 10 split computes the 10-token tail slowly; 261 + 261 keeps both
/// chunks on the batched path. Never returns 0 (safe for `slice::chunks`).
pub fn even_chunk_len(n_tokens: usize, max: usize) -> usize {
    let max = max.max(1);
    let chunks = n_tokens.div_ceil(max).max(1);
    n_tokens.div_ceil(chunks).max(1)
}

impl NativeLlama {
    /// Parsed chat templates when `custom_template` is the one they were built from, else null
    /// (the caller then renders with a one-off parse, the previous behaviour).
    pub fn chat_templates_for(&self, custom_template: Option<&str>) -> *const std::ffi::c_void {
        if custom_template == self.chat_templates_src.as_deref() {
            self.chat_templates
        } else {
            std::ptr::null()
        }
    }

    /// Chunk length for prefilling `n_tokens`: at most one micro-batch (and one batch), split evenly.
    pub fn prompt_chunk_len(&self, n_tokens: usize) -> usize {
        let n_ubatch = unsafe { llama_cpp::llama_n_ubatch(self.ctx_ptr) };
        even_chunk_len(n_tokens, n_ubatch.min(self.n_batch) as usize)
    }

    /// Starts a new timing window (call before a generation).
    pub fn reset_timings(&mut self) {
        unsafe { llama_cpp::llama_perf_context_reset(self.ctx_ptr) }
    }

    /// (description, size in bytes, parameter count) as llama.cpp reports them, like llama-bench's
    /// "model", "size" and "params" columns.
    pub fn describe(&self) -> (String, u64, u64) {
        unsafe {
            let mut buf = vec![0u8; 256];
            let n = llama_cpp::llama_model_desc(self.model_ptr, buf.as_mut_ptr() as *mut c_char, buf.len());
            buf.truncate(n.clamp(0, 255) as usize);
            (
                String::from_utf8_lossy(&buf).into_owned(),
                llama_cpp::llama_model_size(self.model_ptr),
                llama_cpp::llama_model_n_params(self.model_ptr),
            )
        }
    }

    /// llama-bench's test on this context: one warmup, then `reps` runs of `n_prompt` random tokens
    /// decoded in `n_batch` chunks followed by `n_gen` single-token decodes, the memory cleared
    /// before each run. Returns the wall time of each run in nanoseconds (same clock window as llama-bench).
    pub fn bench(&mut self, n_prompt: usize, n_gen: usize, reps: usize) -> anyhow::Result<Vec<u64>> {
        // The benchmark replaces the KV: no checkpoint of a chat stays valid
        self.checkpoints.clear();
        if n_prompt + n_gen > self.n_ctx as usize {
            return Err(anyhow::anyhow!(
                "{} prompt + {} generated tokens do not fit the context of {} tokens", n_prompt, n_gen, self.n_ctx
            ));
        }
        unsafe {
            let vocab = llama_cpp::llama_model_get_vocab(self.model_ptr);
            let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab).max(1) as u64;
            let bos = llama_cpp::llama_vocab_get_add_bos(vocab).then(|| llama_cpp::llama_vocab_bos(vocab));
            let mut rng = 0x2545_F491_4F6C_DD1D_u64;
            let mut random_token = || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                (rng % n_vocab) as i32
            };
            let ctx = self.ctx_ptr;
            let n_batch = self.n_batch.max(1) as usize;
            let decode = |tokens: &mut [i32]| -> anyhow::Result<()> {
                let batch = llama_cpp::llama_batch_get_one(tokens.as_mut_ptr(), tokens.len() as i32);
                match llama_cpp::llama_decode(ctx, batch) {
                    0 => Ok(()),
                    rc => Err(anyhow::anyhow!("llama_decode failed with code {}", rc)),
                }
            };
            let mut run = |n_prompt: usize, n_gen: usize| -> anyhow::Result<()> {
                let mut done = 0;
                while done < n_prompt {
                    let n = n_batch.min(n_prompt - done);
                    let mut tokens: Vec<i32> = (0..n).map(|_| random_token()).collect();
                    if done == 0 {
                        if let Some(b) = bos { tokens[0] = b; }
                    }
                    decode(&mut tokens)?;
                    done += n;
                }
                if n_prompt > 0 {
                    llama_cpp::llama_synchronize(ctx);
                }
                let mut token = bos.unwrap_or_else(|| random_token());
                for _ in 0..n_gen {
                    decode(std::slice::from_mut(&mut token))?;
                    llama_cpp::llama_synchronize(ctx);
                    token = random_token();
                }
                Ok(())
            };
            let mem = llama_cpp::llama_get_memory(ctx);
            // Warmup like llama-bench: a short prompt and one generated token
            llama_cpp::llama_memory_clear(mem, false);
            run(n_prompt.min(n_batch).min(32), n_gen.min(1))?;
            let mut samples = Vec::with_capacity(reps);
            for _ in 0..reps.max(1) {
                llama_cpp::llama_memory_clear(mem, false);
                let started = std::time::Instant::now();
                run(n_prompt, n_gen)?;
                samples.push(started.elapsed().as_nanos() as u64);
            }
            llama_cpp::llama_memory_clear(mem, true);
            Ok(samples)
        }
    }

    /// llama.cpp timing counters since the last `reset_timings`.
    pub fn timings(&self) -> GenerationTimings {
        let d = unsafe { llama_cpp::llama_perf_context(self.ctx_ptr) };
        let per_second = |n: i32, ms: f64| if ms > 0.0 { n as f64 * 1000.0 / ms } else { 0.0 };
        GenerationTimings {
            prompt_n: d.n_p_eval,
            prompt_ms: d.t_p_eval_ms,
            prompt_per_second: per_second(d.n_p_eval, d.t_p_eval_ms),
            predicted_n: d.n_eval,
            predicted_ms: d.t_eval_ms,
            predicted_per_second: per_second(d.n_eval, d.t_eval_ms),
        }
    }
}



/// 🤫 Mute verbose native logs to prevent TUI visual noise.
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

/// llama.cpp / ggml log lines go to the engine log (tracing), never straight to the terminal.
/// ggml_log_level: 4 = error, 3 = warn, everything else (info, debug, line continuation) = debug.
extern "C" fn llama_log_callback(
    level: std::ffi::c_int,
    text: *const std::ffi::c_char,
    _user_data: *mut std::ffi::c_void,
) {
    unsafe {
        if text.is_null() {
            return;
        }
        let c_str = std::ffi::CStr::from_ptr(text);
        let s = c_str.to_string_lossy();
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "." || NOISE_PATTERNS.iter().any(|&p| trimmed.contains(p)) {
            return;
        }
        match level {
            4 => tracing::error!("[llama.cpp] {}", trimmed),
            3 => tracing::warn!("[llama.cpp] {}", trimmed),
            _ => tracing::debug!("[llama.cpp] {}", trimmed),
        }
    }
}

/// mtmd / CLIP (image and audio encoder) log lines go to the engine log like llama.cpp's. Its debug
/// lines that carry the prompt text (`add_text: ...`) are dropped: a chat never reaches a log.
extern "C" fn mtmd_log_callback(
    level: std::ffi::c_int,
    text: *const std::ffi::c_char,
    user_data: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    // SAFETY: mtmd passes a NUL-terminated line valid for this call.
    let line = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy();
    let trimmed = line.trim_start();
    if trimmed.starts_with("add_text") || trimmed.contains("callback returned text") {
        return;
    }
    llama_log_callback(level, text, user_data);
}

/// Sends llama.cpp / ggml and mtmd / CLIP logs to tracing. Call before `llama_backend_init` so
/// backend start-up lines (e.g. `ggml_cuda_init`) do not go straight to the terminal.
pub fn route_llama_logs() {
    unsafe {
        llama_cpp::llama_log_set(Some(llama_log_callback), std::ptr::null_mut());
        crate::ffi::mtmd::mtmd_helper_log_set(Some(mtmd_log_callback), std::ptr::null_mut());
    }
}

pub fn print_memory_trace(step: &str) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let free_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    tracing::debug!(
        "🆘 [MEMORY TRACE: {}] Free RAM: {:.2} GB / {:.2} GB",
        step, free_ram_gb, total_ram_gb
    );
}

impl NativeLlama {
    /// 🧬 Load a model and initialize context with industrial booster params.
    pub fn load(
        model_path: &str,
        model_params: LlamaModelParams,
        mut ctx_params: LlamaContextParams,
        dna: &mut StructuralDNA,
        kv_cache_quantization_mode: u8,
        context_shifting_mode: u8,
        speculative_decoding_mode: u8,
        moe_controller: Option<Arc<std::sync::Mutex<()>>>,
    ) -> anyhow::Result<Self> {
        route_llama_logs();

        // ══ HARDWARE OPTIMIZATION (Hardware Overrides) ══
        // We now use llama_log_callback to pipe all logs instead of swallowing them.

        // 🚀 Backend Init: Already handled globally by cluaiz_kernel_init() in ffi_exports.rs.
        // DO NOT call llama_backend_init() here — it WIPES existing CUDA registration.
        // DO NOT register CUDA here — it causes VRAM allocation even in CPU mode (n_gpu_layers=0)
        // because #[cfg(feature = "cuda")] is compile-time, not runtime.

        let c_path = CString::new(model_path)?;

        let model_size_bytes = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
        let model_size_gb = model_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let available_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

        // 🛡️ Fail-Fast Pre-Allocation Gate:
        // If model is targeting CPU (n_gpu_layers == 0) and exceeds physical available RAM,
        // abort immediately. Calling llama.cpp will attempt to allocate a CPU_REPACK buffer
        // triggering Windows pagefile thrashing and 100% SSD lockup.
        // An mmap load without repack copies nothing into RAM (the planner's disk-read mode), so it passes.
        let reads_from_disk = model_params.is_mmap() && !model_params.use_extra_bufts;
        if model_params.n_gpu_layers == 0 && model_size_gb > available_ram_gb && !reads_from_disk {
            tracing::error!(
                "❌ [Native-Llama] Model size ({:.2} GB) exceeds available system RAM ({:.2} GB). \
                 Refusing CPU load to prevent SSD pagefile thrashing and lockup.",
                model_size_gb, available_ram_gb
            );
            return Err(anyhow::anyhow!(
                "Insufficient RAM to load model on CPU. Model: {:.2} GB, Available RAM: {:.2} GB. SSD thrashing prevented.",
                model_size_gb, available_ram_gb
            ));
        }

        print_memory_trace("1. BEFORE MODEL LOAD");
        tracing::info!("📊 [Native-Llama] FFI Parameters: n_gpu_layers = {}, load_mode = {}, n_threads = {}, n_threads_batch = {}", model_params.n_gpu_layers, model_params.load_mode, ctx_params.n_threads, ctx_params.n_threads_batch);
        info!(
            "🧬 [Native-Llama] Loading model: {} | ctx: {} tokens",
            model_path, ctx_params.n_ctx
        );
        let mut n_gpu_layers_loaded = model_params.n_gpu_layers;
        let mut model_ptr =
            unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), model_params) };

        print_memory_trace("2. AFTER MODEL LOAD");

        // 🔒 Mlock Graceful Fallback
        if model_ptr.is_null() && model_params.is_mlock() {
            warn!("🔒 [Arbiter] mlock failed. Falling back to high-speed mmap...");
            let mut fallback_params = model_params;
            fallback_params.set_mlock(false);
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), fallback_params) };
        }

        // 🛡️ CERD DOCTRINE GPU-FALLBACK (No Hardcoded Strings)
        // If Model Load fails with n_gpu_layers != 0 (e.g. -1 for all layers, or >0), the CUDA backend
        // likely doesn't support the tensor format (e.g., TQ1_0 or TQ2_0 BitNet models).
        // We must gracefully fallback to CPU-only.
        if model_ptr.is_null() && model_params.n_gpu_layers > 1 {
            tracing::info!("⚠️ [Native-Llama] VRAM allocation pressure during model load. Clamping n_gpu_layers to {} and retrying...", model_params.n_gpu_layers / 2);
            let mut half_params = model_params;
            half_params.n_gpu_layers /= 2;
            n_gpu_layers_loaded = half_params.n_gpu_layers;
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), half_params) };
        }

        // 🛡️ CERD DOCTRINE GPU-FALLBACK (No Hardcoded Strings)
        // If Model Load fails with n_gpu_layers != 0 (e.g. -1 for all layers, or >0), the CUDA backend
        // likely doesn't support the tensor format (e.g., TQ1_0 or TQ2_0 BitNet models).
        // We must gracefully fallback to CPU-only if RAM allows.
        if model_ptr.is_null() && model_params.n_gpu_layers != 0 {
            if model_size_gb > available_ram_gb {
                tracing::error!(
                    "❌ [Native-Llama] Model load failed on GPU and model size ({:.2} GB) exceeds available RAM ({:.2} GB). Refusing CPU fallback to prevent SSD thrashing.",
                    model_size_gb, available_ram_gb
                );
                return Err(anyhow::anyhow!(
                    "Model failed on GPU and exceeds available RAM ({:.2} GB > {:.2} GB). CPU fallback aborted to prevent SSD thrashing.",
                    model_size_gb, available_ram_gb
                ));
            }
            tracing::warn!("⚠️ [Native-Llama] Model Load Failed on GPU. Falling back to CPU-only...");
            let mut cpu_params = model_params;
            cpu_params.n_gpu_layers = 0; // Force CPU
            n_gpu_layers_loaded = 0;
            cpu_params.no_host = false; // CRITICAL: Must allow host memory allocation for CPU inference!
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), cpu_params) };
        }

        // 🚨 Ultimate Mmap Fallback
        // If it STILL fails on CPU, it might be an mmap mapping limitation (e.g. Windows file locking or unsupported tensor alignment).
        if model_ptr.is_null() && model_params.is_mmap() {
            // Check available system RAM first to prevent disk thrashing OOM
            let model_size_bytes = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
            let model_size_gb = model_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            let available_ram_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

            if model_size_gb > available_ram_gb {
                tracing::error!(
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

            tracing::warn!("⚠️ [Native-Llama] Model Load Failed with mmap. Falling back to RAM allocation (use_mmap = false)...");
            let mut ram_params = model_params;
            ram_params.n_gpu_layers = 0;
            n_gpu_layers_loaded = 0;
            ram_params.no_host = false;
            ram_params.set_mmap(false);
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), ram_params) };
        }

        if model_ptr.is_null() {
            return Err(anyhow::anyhow!("Model Load Failure: {}", model_path));
        }

        let model_dir = std::path::Path::new(model_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        tracing::info!(
            "🧬 [Native-Llama] Starting DNA Discovery for: {:?}",
            model_dir
        );

        dna.weights_already_loaded = true;

        // Save the requested context size to prevent it from being overwritten by the global GPU VRAM arbiter in CPU-only mode
        let requested_n_ctx = ctx_params.n_ctx;

        if let Err(e) = dna.discover_from_path(model_dir) {
            tracing::info!("⚠️ [Native-Llama] DNA Discovery Failed: {}", e);
        }

        let (native_start, native_end) = crate::native::templater::extract_thinking_tags_native(
            model_ptr,
            dna.chat_template.as_deref(),
        );
        dna.think_tag_schema = native_start.clone().unwrap_or_default();
        dna.think_end_schema = native_end.clone().unwrap_or_default();
        if let Some(st) = native_start {
            info!(
                "🧠 [Native-Llama] llama.cpp detected native thinking start tag: {:?}",
                st
            );
        }
        if let Some(et) = native_end {
            info!(
                "🧠 [Native-Llama] llama.cpp detected native thinking end tag: {:?}",
                et
            );
        }

        // Ensure n_ctx is strictly bound by Negotiator's requested limit, preventing 256k token (14.4 GB) KV Cache allocations
        ctx_params.n_ctx = requested_n_ctx;
        info!(
            "🎯 [Native-Llama] Hardware Handshake: Context Window strictly locked to: {} tokens",
            ctx_params.n_ctx
        );

        let mut speculative_decoding_mode = speculative_decoding_mode;
        if dna.requires_fp16_kv() || !dna.supports_flash_attention() {
            info!("🛡️ [Native-Llama] Non-standard attention / recurrent architecture: Disabling speculative decoding.");
            speculative_decoding_mode = 0;
        }

        unsafe {
            // Ensure CUDA graphs remain enabled for maximum GPU inference throughput.
            std::env::remove_var("GGML_CUDA_DISABLE_GRAPHS");
        }

        // KV placement and op offload come from the plan (caller's params); only logged here
        if model_params.n_gpu_layers != 0 {
            info!("🧠 [Native-Llama] op_offload = {}", ctx_params.op_offload);
            if ctx_params.offload_kqv == 1 {
                info!("🧠 [Native-Llama] KV Cache offloaded to dedicated GPU VRAM (Single Source of Truth).");
            } else {
                info!("🧠 [Native-Llama] KV Cache placed in System Host RAM (Single Source of Truth).");
            }
        }

        // 🛡️ Disable C-FFI per-tensor eval callback to eliminate CUDA Pointer Desynchronization
        // and OS interrupt thrashing. Model stays fully resident in statically assigned VRAM/RAM.
        ctx_params.cb_eval = std::ptr::null_mut();
        ctx_params.cb_eval_user_data = std::ptr::null_mut();

        // 🛡️ Dynamic Model Header Truth (Native C++ Engine & Tensor Geometry)
        let is_pure_recurrent = unsafe { llama_cpp::llama_model_is_recurrent(model_ptr) };

        info!("🔍 [Native-Llama] Model Header Truth: is_pure_recurrent={}", is_pure_recurrent);

        // Flash attention comes from the caller's params (user setting / request); only recurrent models force it off
        if is_pure_recurrent {
            info!("🛡️ [Native-Llama] Pure recurrent architecture (SSM/Mamba/RWKV): Disabling Flash Attention.");
            ctx_params.flash_attn_type = 0;
            speculative_decoding_mode = 0;
            ctx_params.type_k = 1; // GGML_TYPE_F16
            ctx_params.type_v = 1; // GGML_TYPE_F16
        }
        info!(
            "⚡ [Native-Llama] flash_attn_type={} (-1 auto, 0 off, 1 on), threads {}/{}",
            ctx_params.flash_attn_type, ctx_params.n_threads, ctx_params.n_threads_batch
        );

        print_memory_trace("4. BEFORE CONTEXT CREATION");
        let mut ctx_ptr = unsafe { llama_cpp::llama_init_from_model(model_ptr, ctx_params) };
        print_memory_trace("5. AFTER CONTEXT CREATION");

        // 🛡️ CERD DOCTRINE FA-FALLBACK
        // If Context Init fails with Flash Attention, fallback without Flash Attention (enforcing F16 V-cache as required by llama.cpp)
        if ctx_ptr.is_null() && ctx_params.flash_attn_type > 0 {
            tracing::info!("⚠️ [Native-Llama] Context Init Failed with Flash Attention ON. Retrying with fallback...");
            let mut fallback_ctx_params = ctx_params;
            fallback_ctx_params.flash_attn_type = 0;
            fallback_ctx_params.type_v = 1; // Upstream llama.cpp requires F16 V-cache when FA is disabled
            ctx_ptr = unsafe { llama_cpp::llama_init_from_model(model_ptr, fallback_ctx_params) };
        }

        if ctx_ptr.is_null() {
            unsafe { llama_cpp::llama_model_free(model_ptr) };
            return Err(anyhow::anyhow!("Context Init Failure"));
        }

        let mut llama = Self {
            model_ptr,
            ctx_ptr,
            interrupt_signal: Arc::new(AtomicBool::new(false)),
            n_ctx: ctx_params.n_ctx,
            n_batch: if ctx_params.n_ctx == 0 {
                ctx_params.n_batch
            } else {
                std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch)
            },
            kv_cache_quantization_mode,
            context_shifting_mode,
            speculative_decoding_mode,
            moe_controller,
            n_gpu_layers_loaded,
            chat_templates: crate::native::templater::parse_templates(model_ptr, dna.chat_template.as_deref()),
            chat_templates_src: dna.chat_template.clone(),
            last_thinking: None,
            last_context: None,
            checkpoints: Vec::new(),
        };
        llama.warmup();
        Ok(llama)
    }

    /// llama.cpp's own load-time warmup (`common_init_from_params`): one decode of BOS + EOS, then the
    /// memory is cleared. Weights get paged in and GPU kernels initialised during load instead of on
    /// the user's first message. A failure only logs: the model itself is loaded and usable.
    fn warmup(&mut self) {
        unsafe {
            // Encoder-decoder models need an encode pass first (llama.cpp does that); not used for chat here
            if llama_cpp::llama_model_has_encoder(self.model_ptr) {
                return;
            }
            let vocab = llama_cpp::llama_model_get_vocab(self.model_ptr);
            // LLAMA_TOKEN_NULL (-1) = the model has no such token
            let mut tokens: Vec<i32> = [llama_cpp::llama_vocab_bos(vocab), llama_cpp::llama_vocab_eos(vocab)]
                .into_iter()
                .filter(|t| *t >= 0)
                .collect();
            if tokens.is_empty() {
                tokens.push(0);
            }
            tokens.truncate(self.n_batch.max(1) as usize);
            let batch = llama_cpp::llama_batch_get_one(tokens.as_mut_ptr(), tokens.len() as i32);
            if llama_cpp::llama_decode(self.ctx_ptr, batch) != 0 {
                warn!("⚠️ [Native-Llama] Warmup decode failed; the first message will pay the warmup cost");
            }
            llama_cpp::llama_memory_clear(llama_cpp::llama_get_memory(self.ctx_ptr), true);
            llama_cpp::llama_synchronize(self.ctx_ptr);
            llama_cpp::llama_perf_context_reset(self.ctx_ptr);
        }
    }

    pub fn resize_context(&mut self, mut ctx_params: LlamaContextParams) -> anyhow::Result<()> {
        if self.model_ptr.is_null() {
            return Err(anyhow::anyhow!("Cannot resize context: Model not loaded"));
        }
        // 🛡️ Skip redundant context re-allocations if parameters are unchanged
        if self.n_ctx == ctx_params.n_ctx
            && self.n_batch == ctx_params.n_batch
            && !self.ctx_ptr.is_null()
        {
            return Ok(());
        }

        let is_pure_recurrent = unsafe { llama_cpp::llama_model_is_recurrent(self.model_ptr) };

        if is_pure_recurrent {
            ctx_params.flash_attn_type = 0;
            ctx_params.type_k = 1;
            ctx_params.type_v = 1;
        } else {
            ctx_params.flash_attn_type = -1; // Auto (probed per layer by llama.cpp)
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
            self.n_batch = if ctx_params.n_ctx == 0 {
                ctx_params.n_batch
            } else {
                std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch)
            };
        }
        Ok(())
    }

    pub fn stitch_signal(
        &mut self,
        signal_id: i32,
        _offset: i32,
        length: i32,
    ) -> anyhow::Result<()> {
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
            // 🧹 KV Cache Flush: Ensure KV cache is clear before starting new prefill
            let mem = llama_cpp::llama_get_memory(self.ctx_ptr);
            llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);

            let vocab = llama_cpp::llama_model_get_vocab(self.model_ptr);
            let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

            if n_vocab <= 0 {
                return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
            }

            let c_prompt = std::ffi::CString::new(prompt.to_string())?;

            // 1. Tokenize
            tracing::info!(
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
                tracing::error!("❌ [Native-Llama] Tokenization failed!");
                return Err(anyhow::anyhow!("Tokenization failed"));
            }
            tokens.truncate(n_tokens as usize);
            tracing::debug!(
                "🧠 [Native-Llama] Tokenization successful: {} tokens",
                tokens.len()
            );

            // 2. Initial Batch Decode (Prefill) with Chunking
            let chunk_size = self.n_batch as i32;
            tracing::debug!(
                "🧠 [Native-Llama] Initializing llama batch of size {}...",
                chunk_size
            );
            let mut batch = llama_cpp::llama_batch_init(chunk_size, 0, 1);

            let mut tokens_processed = 0;
            let step = self.prompt_chunk_len(tokens.len());

            while tokens_processed < tokens.len() {
                let current_chunk = std::cmp::min(step, tokens.len() - tokens_processed);

                for i in 0..current_chunk {
                    let global_i = tokens_processed + i;
                    *batch.token.add(i) = tokens[global_i];
                    *batch.pos.add(i) = global_i as i32;
                    *batch.n_seq_id.add(i) = 1;
                    *(*batch.seq_id.add(i)).add(0) = 0;
                    *batch.logits.add(i) = if global_i == tokens.len() - 1 { 1 } else { 0 };
                }
                batch.n_tokens = current_chunk as i32;

                tracing::debug!(
                    "🧠 [Native-Llama] Prefilling chunk of {} tokens ({} / {})...",
                    current_chunk,
                    tokens_processed + current_chunk,
                    tokens.len()
                );
                if llama_cpp::llama_decode(self.ctx_ptr, batch) != 0 {
                    tracing::error!("❌ [Native-Llama] llama_decode failed!");
                    llama_cpp::llama_batch_free(batch);
                    return Err(anyhow::anyhow!(
                        "Prefill decode failed at chunk starting at {}",
                        tokens_processed
                    ));
                }
                tracing::debug!("🧠 [Native-Llama] Decoded chunk successfully");

                tokens_processed += current_chunk;
            }

            tracing::debug!("🧠 [Native-Llama] Prefill complete, freeing batch...");
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
            // Parsed templates reference the model's vocab: free them first
            if !self.chat_templates.is_null() {
                llama_cpp::llama_chat_templates_handle_free(self.chat_templates);
            }
            if !self.ctx_ptr.is_null() {
                llama_cpp::llama_free(self.ctx_ptr);
            }
            if !self.model_ptr.is_null() {
                llama_cpp::llama_model_free(self.model_ptr);
            }
        }
    }
}

// SAFETY: NativeLlama encapsulates raw FFI pointers (*mut c_void) to llama.cpp model and context.
// 1. Send: Ownership can be transferred safely across threads because the underlying C pointers
//    are not tied to thread-local storage and remain valid until NativeLlama is dropped.
// 2. Sync: Implementing Sync is sound because all inference, decoding, sampling, and mutation
//    methods (stream_tokens, prefill, stitch_signal, load_prompt_cache) strictly require
//    exclusive mutable references (&mut self). Rust's static borrow checker guarantees that
//    concurrent threads cannot invoke mutating FFI calls on the underlying llama_context simultaneously.
unsafe impl Send for NativeLlama {}
unsafe impl Sync for NativeLlama {}

#[cfg(test)]
mod tests {
    use super::even_chunk_len;

    #[test]
    fn prompt_chunks_are_even_and_never_exceed_the_micro_batch() {
        // the case from the benchmark: 522 tokens, ubatch 512 -> 261 + 261 instead of 512 + 10
        assert_eq!(even_chunk_len(522, 512), 261);
        // fits in one micro-batch -> one chunk; exact multiples stay full
        assert_eq!(even_chunk_len(300, 512), 300);
        assert_eq!(even_chunk_len(1024, 512), 512);
        // never 0, even for empty input or a broken max
        assert_eq!(even_chunk_len(0, 512), 1);
        assert_eq!(even_chunk_len(10, 0), 1);
        for n in 1..3000usize {
            for max in [1usize, 7, 32, 256, 512, 2048] {
                let len = even_chunk_len(n, max);
                assert!(len >= 1 && len <= max, "n {} max {} -> {}", n, max, len);
                let chunks = n.div_ceil(len);
                assert_eq!(chunks, n.div_ceil(max), "n {} max {}: chunk count must not grow", n, max);
                assert!(n - (chunks - 1) * len >= len.saturating_sub(chunks), "n {} max {}: uneven tail", n, max);
            }
        }
    }
}
