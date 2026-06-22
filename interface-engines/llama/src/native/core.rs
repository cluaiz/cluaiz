use crate::ffi::llama_cpp::{self, LlamaContextParams, LlamaModelParams};
use cluaize_shared::StructuralDNA;
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
}

/// 🤫 Sovereign Silence: Mute verbose native logs to prevent TUI visual noise.
#[allow(dead_code)]
extern "C" fn silent_llama_log(
    _level: i32,
    _text: *const c_char,
    _user_data: *mut std::ffi::c_void,
) {
}

impl NativeLlama {
    /// 🧬 Load a model and initialize context with industrial booster params.
    pub fn load(
        model_path: &str,
        model_params: LlamaModelParams,
        mut ctx_params: LlamaContextParams,
        dna: &mut cluaize_shared::metadata::dna::StructuralDNA,
        kv_cache_quantization_mode: u8,
        context_shifting_mode: u8,
        speculative_decoding_mode: u8,
    ) -> anyhow::Result<Self> {
        // ══ SOVEREIGN OPTIMIZATION (Hardware Overrides) ══
        std::env::set_var("GGML_LOG_LEVEL", "INFO");

        // Register default callback
        unsafe { llama_cpp::llama_log_set(None, std::ptr::null_mut()) };

        // 🚀 Step 1: Initialize all native backends (loads CPU, etc.)
        unsafe { llama_cpp::llama_backend_init() };

        // 🚀 Step 2: AFTER init, manually inject CUDA backend so it isn't overwritten
        #[cfg(feature = "cuda")]
        unsafe {
            if model_params.n_gpu_layers != 0 {
                eprintln!("🔥 [Sovereign-Llama] Manually injecting CUDA backend AFTER llama_backend_init...");
                let reg = llama_cpp::ggml_backend_cuda_reg();
                if !reg.is_null() {
                    llama_cpp::ggml_backend_register(reg);
                    eprintln!("✅ [Sovereign-Llama] CUDA backend registered successfully!");
                } else {
                    eprintln!(
                        "❌ [Sovereign-Llama] CUDA reg pointer is NULL! Check CUDA runtime DLLs."
                    );
                }
            } else {
                eprintln!(
                    "❄️ [Sovereign-Llama] CPU-only mode selected. Skipping CUDA backend injection."
                );
            }
        }

        let c_path = CString::new(model_path)?;

        println!("📊 [Native-Llama] FFI Parameters: n_gpu_layers = {}, use_mmap = {}, n_threads = {}, n_threads_batch = {}", model_params.n_gpu_layers, model_params.use_mmap, ctx_params.n_threads, ctx_params.n_threads_batch);
        info!(
            "🧬 [Native-Llama] Loading model: {} | ctx: {} tokens",
            model_path, ctx_params.n_ctx
        );
        let mut model_ptr =
            unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), model_params) };

        // 🔒 Mlock Graceful Fallback
        if model_ptr.is_null() && model_params.use_mlock {
            warn!("🔒 [Arbiter] mlock failed. Falling back to high-speed mmap...");
            let mut fallback_params = model_params;
            fallback_params.use_mlock = false;
            model_ptr =
                unsafe { llama_cpp::llama_model_load_from_file(c_path.as_ptr(), fallback_params) };
        }

        if model_ptr.is_null() {
            return Err(anyhow::anyhow!("Model Load Failure: {}", model_path));
        }

        let model_dir = std::path::Path::new(model_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        eprintln!(
            "🧬 [Native-Llama] Starting DNA Discovery for: {:?}",
            model_dir
        );

        dna.weights_already_loaded = true;

        // Save the requested context size to prevent it from being overwritten by the global GPU VRAM arbiter in CPU-only mode
        let requested_n_ctx = ctx_params.n_ctx;

        if let Err(e) = dna.discover_from_path(model_dir) {
            eprintln!("⚠️ [Native-Llama] DNA Discovery Failed: {}", e);
        }

        if model_params.n_gpu_layers == 0 {
            // Restore requested context size for CPU-only mode to prevent GPU VRAM capping
            dna.max_context_length = Some(requested_n_ctx as usize);
        }

        if let Some(ctx) = dna.max_context_length {
            info!(
                "🎯 [Native-Llama] SOVEREIGN HANDSHAKE: Setting n_ctx = {} (DNA Truth)",
                ctx
            );
            ctx_params.n_ctx = ctx as u32;
        }

        let mut speculative_decoding_mode = speculative_decoding_mode;
        if dna.model_identity.to_lowercase().contains("gemma") {
            info!("🛡️ [Native-Llama] Gemma model detected: Disabling speculative decoding.");
            speculative_decoding_mode = 0;
        }

        if speculative_decoding_mode == 1 || speculative_decoding_mode == 2 {
            std::env::set_var("GGML_CUDA_USE_GRAPHS", "0");
        } else {
            std::env::set_var("GGML_CUDA_USE_GRAPHS", "1");
        }

        let ctx_ptr = unsafe { llama_cpp::llama_init_from_model(model_ptr, ctx_params) };

        if ctx_ptr.is_null() {
            unsafe { llama_cpp::llama_model_free(model_ptr) };
            return Err(anyhow::anyhow!("Context Init Failure"));
        }

        Ok(Self {
            model_ptr,
            ctx_ptr,
            interrupt_signal: Arc::new(AtomicBool::new(false)),
            n_ctx: ctx_params.n_ctx,
            n_batch: std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch),
            kv_cache_quantization_mode,
            context_shifting_mode,
            speculative_decoding_mode,
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
            self.n_batch = std::cmp::min(ctx_params.n_ctx, ctx_params.n_batch);
        }
        Ok(())
    }

    pub fn stitch_signal(&self, signal_id: i32, offset: i32, length: i32) -> anyhow::Result<()> {
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
    pub fn load_prompt_cache(&self, path: &str) -> anyhow::Result<Vec<i32>> {
        info!("💾 [Native-Llama] Loading prompt cache from: {}", path);
        let c_path = std::ffi::CString::new(path)?;
        let mut tokens = vec![0i32; 8192];
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
    pub fn prefill_prompt(&self, prompt: &str) -> anyhow::Result<Vec<i32>> {
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
            println!(
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
        &self,
        prompt: &str,
        max_tokens: usize,
        dna: &StructuralDNA,
        last_prefilled_tokens: &[i32],
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
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
