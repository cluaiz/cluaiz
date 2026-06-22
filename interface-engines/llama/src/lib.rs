#![allow(warnings)]
//! Sovereign Implementation B: Accelerated Feature-Based Runtime (Llama Engine).
//! This kernel is loaded dynamically by the SiliconOrchestrator.

use anyhow::Result;
use cluaize_shared::{CluaizeContext, CluaizeInference, UnifiedBackend};
use neural_core::interfaces::memory_contract::SovereignBuffer;
use std::sync::Arc;
use tokenizers::Tokenizer;

pub mod asm_kernels;
pub mod bridge;
pub mod config;
pub mod ffi;
pub mod hybrid;
pub mod loader;
pub mod native;
pub mod pipeline;
pub mod router;
pub mod sampling;

use crate::config::BoosterConfig;
use crate::native::NativeLlama;

// ─── FFI Helpers ───────────────────────────────────────────────────────────

#[repr(C)]
struct CallbackWrapper {
    callback: extern "C" fn(*const std::os::raw::c_char, *mut std::ffi::c_void) -> bool,
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for CallbackWrapper {}
unsafe impl Sync for CallbackWrapper {}

pub use asm_kernels::BareMetalMath;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::llama_cpp::{self, LlamaContextParams, LlamaModelParams};

    #[test]
    fn verify_struct_sizes() {
        println!(
            "📊 [FFI-Verify] Size of LlamaContextParams: {}",
            std::mem::size_of::<LlamaContextParams>()
        );
        println!(
            "📊 [FFI-Verify] Size of LlamaModelParams: {}",
            std::mem::size_of::<LlamaModelParams>()
        );

        let dummy: LlamaContextParams = unsafe { std::mem::zeroed() };
        let base = &dummy as *const _ as usize;
        println!(
            "📊 [FFI-Verify] Offset of n_ctx: {}",
            (&dummy.n_ctx as *const _ as usize) - base
        );
        println!(
            "📊 [FFI-Verify] Offset of flash_attn_type: {}",
            (&dummy.flash_attn_type as *const _ as usize) - base
        );
        println!(
            "📊 [FFI-Verify] Offset of rope_freq_base: {}",
            (&dummy.rope_freq_base as *const _ as usize) - base
        );
        println!(
            "📊 [FFI-Verify] Offset of cb_eval: {}",
            (&dummy.cb_eval as *const _ as usize) - base
        );
        println!(
            "📊 [FFI-Verify] Offset of embeddings: {}",
            (&dummy.embeddings as *const _ as usize) - base
        );
        println!(
            "📊 [FFI-Verify] Offset of samplers: {}",
            (&dummy.samplers as *const _ as usize) - base
        );

        let defaults = unsafe { llama_cpp::llama_context_default_params() };
        println!("📊 [FFI-Verify] Default n_ctx: {}", defaults.n_ctx);
        println!("📊 [FFI-Verify] Default n_batch: {}", defaults.n_batch);
        println!("📊 [FFI-Verify] Default n_ubatch: {}", defaults.n_ubatch);
        println!("📊 [FFI-Verify] Default n_seq_max: {}", defaults.n_seq_max);
        println!(
            "📊 [FFI-Verify] Default flash_attn_type: {}",
            defaults.flash_attn_type
        );
        println!("📊 [FFI-Verify] Default n_threads: {}", defaults.n_threads);
        println!(
            "📊 [FFI-Verify] Default rope_freq_base: {}",
            defaults.rope_freq_base
        );
        println!(
            "📊 [FFI-Verify] Default embeddings: {}",
            defaults.embeddings
        );

        println!("🔍 [Memory-Probe] Dumping raw bytes of LlamaContextParams defaults:");
        let ptr = &defaults as *const _ as *const u32;
        for i in 0..32 {
            let val = unsafe { *ptr.add(i) };
            println!(
                "  [{:02}] Offset {:03}: 0x{:08x} ({})",
                i,
                i * 4,
                val,
                val as i32
            );
        }
    }
}

pub struct RuntimeB {
    pub model_path: String,
    pub context: CluaizeContext,
    pub booster: BoosterConfig,
    pub native: Option<NativeLlama>,
    pub lucebox: Option<Arc<ffi::lucebox::LuceboxBridge>>,
    pub last_prefilled_tokens: Vec<i32>,
}

impl RuntimeB {
    pub fn new(path: &str, context: CluaizeContext) -> Self {
        Self {
            model_path: path.to_string(),
            context,
            booster: BoosterConfig::default(),
            native: None,
            lucebox: None,
            last_prefilled_tokens: Vec::new(),
        }
    }

    /// 🧬 Load the model natively into memory using current booster settings.
    pub fn load_native(&mut self) -> anyhow::Result<()> {
        let model_params = self.booster.to_model_params();

        // 🧬 DNA TRUTH SYNC: Ensure DNA context is applied to context params
        let mut ctx_params = self.booster.to_context_params();

        if let Some(ctx) = self.context.dna.max_context_length {
            ctx_params.n_ctx = ctx as u32;
        }

        // 🧠 RESOLVE SPECULATIVE MODE & SYNC DNA
        // We probe GGUF metadata + tensor names to detect hybrid/recurrent models (e.g. Qwen3.5 GDN).
        // GGUFProber now checks: architecture name, *.layer_types metadata, AND tensor patterns.
        let (has_native_mtp, is_ssm_model) = if let Ok((metadata, tensor_infos, _)) =
            cluaize_shared::utils::GGUFProber::probe(std::path::Path::new(&self.model_path))
        {
            (
                cluaize_shared::utils::GGUFProber::check_native_mtp(&tensor_infos),
                cluaize_shared::utils::GGUFProber::check_recurrent_ssm(&metadata, &tensor_infos),
            )
        } else {
            (false, false)
        };

        if is_ssm_model {
            // 🚨 For hybrid/recurrent models (Qwen3.5 GDN, Mamba, RWKV):
            // Speculative decoding is incompatible with non-transformer architectures.
            eprintln!("⚖️ [Llama-Engine] SSM/Hybrid architecture detected.");
            eprintln!("⚖️ [Llama-Engine] → Speculative Decoding: FORCED OFF");
            self.booster.speculative_decoding = "off".to_string();
            // Note: We DO NOT force context_shifting off here anymore, as it breaks continuous generation.
            // We let system_booster.json decide the context_shifting mode.
        }

        let speculative_mode = if self.booster.speculative_decoding.to_lowercase() != "off" {
            if has_native_mtp {
                "native_mtp"
            } else {
                "eagle"
            }
        } else {
            "off"
        };
        eprintln!(
            "🧠 [Llama-Engine] Dynamic Speculative Sync: Mode resolved as '{}' (booster: {})",
            speculative_mode, self.booster.speculative_decoding
        );
        self.context
            .dna
            .dynamic_attributes
            .insert("speculative_mode".to_string(), speculative_mode.to_string());

        tracing::info!(
            "🧬 [Native-Llama] Loading model: {} | ctx: {} tokens",
            self.model_path,
            ctx_params.n_ctx
        );

        // 🚀 BATCH SYNC: Optimized for 4GB hardware by default, scalable via BoosterConfig.
        // If running in CPU-only mode (n_gpu_layers == 0), force batch size to 32 to prevent GGML graph allocation limits on large contexts.
        if model_params.n_gpu_layers == 0 {
            ctx_params.n_batch = 32;
            ctx_params.n_ubatch = 32;
        } else {
            ctx_params.n_batch = if ctx_params.n_batch == 0 {
                512
            } else {
                ctx_params.n_batch
            };
            ctx_params.n_ubatch = if ctx_params.n_ubatch == 0 {
                512
            } else {
                ctx_params.n_ubatch
            };
        }

        let native = NativeLlama::load(
            &self.model_path,
            model_params,
            ctx_params,
            &mut self.context.dna,
            match self.booster.kv_cache_quantization.to_lowercase().as_str() {
                "kv8" => 1,
                "kv4" => 2,
                _ => 0,
            },
            match self.booster.context_shifting.to_lowercase().as_str() {
                "off" => 0,
                "minimal" => 1,
                "standard" | "auto" | "on" => 2,
                "aggressive" => 3,
                "extreme" => 4,
                _ => 2,
            },
            match self.booster.speculative_decoding.to_lowercase().as_str() {
                "off" => 0,
                "on" => 1,
                _ => 2,
            },
        )?;
        self.native = Some(native);
        tracing::info!("✅ [Llama-Engine] Native Model Loaded & Optimized.");
        Ok(())
    }

    /// 🛠️ Attach the Lucebox accelerator bridge
    pub fn attach_accelerator(&mut self, lib_path: &str) -> anyhow::Result<()> {
        let bridge = ffi::lucebox::LuceboxBridge::load(lib_path)?;
        self.lucebox = Some(Arc::new(bridge));
        tracing::info!("🚀 [Llama-Engine] Lucebox Accelerator Attached.");
        Ok(())
    }
}

impl UnifiedBackend for RuntimeB {
    fn generate(&mut self, prompt: &str, _max_tokens: usize) -> Result<String, String> {
        Ok(format!(
            "Sovereign Llama Engine: Ready for prompt: {}",
            prompt
        ))
    }

    fn prefill(&mut self, prompt: &str) -> Result<()> {
        if let Some(ref native) = self.native {
            let tokens = native.prefill_prompt(prompt)?;
            self.last_prefilled_tokens = tokens;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Native backend not initialized"))
        }
    }

    fn evaluate_tps(&self) -> f64 {
        // 📡 Sovereign Telemetry: Return the real-time TPS from the pulse counter.
        // This counter is incremented for every token generated in native.rs.
        cluaize_shared::hardware::telemetry::get_pulse()
            .tps_counter
            .load(std::sync::atomic::Ordering::Relaxed) as f64
    }
}

impl CluaizeInference for RuntimeB {
    fn forward_raw(&mut self, _input_ids: &[u32], _pos: usize) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("FFI forward optimized via ASM kernels"))
    }

    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        let mut callback = callback;

        // 🛡️ Neural Circuit Breaker: check if paths are safe
        let mut cb = cluaize_shared::hardware::circuit_breaker::NeuralCircuitBreaker::default();
        if !cb.can_proceed() {
            return Err(anyhow::anyhow!(
                "🚨 [Circuit Breaker] Inference blocked due to previous system instability."
            ));
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // 🚀 High-Performance Native Path
            if let Some(ref native) = self.native {
                let res = native.stream_tokens(
                    prompt,
                    max_tokens,
                    &self.context.dna,
                    &self.last_prefilled_tokens,
                    callback,
                );

                if res.is_ok() {
                    cb.record_success();
                } else {
                    cb.record_failure("Native stream error");
                }
                return res;
            }

            // 🛡️ Safe Binary Fallback Path
            tokio::task::block_in_place(|| {
                let handle = tokio::runtime::Handle::current();
                handle
                    .block_on(crate::pipeline::RuntimeBPipeline::execute_stream(
                        &self.model_path,
                        &self.context,
                        prompt,
                        max_tokens,
                        callback,
                    ))
                    .map_err(|e| anyhow::anyhow!(e))
            })
        }));

        let execution_result = match result {
            Ok(res) => res,
            Err(_) => {
                tracing::error!(
                    "🚨 [FFI-Panic] Caught panic in generate_stream! Preventing OS crash."
                );
                Err(anyhow::anyhow!("Kernel panic during stream generation."))
            }
        };
        self.last_prefilled_tokens.clear();
        execution_result
    }

    /// 💉 Neural Injection Hook: Injects multiple pre-encoded signal states into the Llama cache.
    fn inject_signals(
        &mut self,
        signals: Vec<cluaize_shared::hardware::memory::kv_cache::stitching::CluaizeSignal>,
    ) -> Result<()> {
        let max_ctx = self.context.dna.max_context_length.unwrap_or(4096);
        let mut current_offset = 0;

        if signals.is_empty() {
            return Ok(());
        }

        println!(
            "💉 [Llama-Engine] Multi-Signal Injection Active: {} signals detected.",
            signals.len()
        );

        if let Some(ref lucebox) = self.lucebox {
            let max_layers = self.context.dna.layer_count.unwrap_or(32);

            for (i, signal) in signals.iter().enumerate() {
                let token_count = signal.token_count;

                // 🛑 Positional Guard
                if current_offset + token_count > max_ctx {
                    tracing::error!("❌ [Llama-Engine] Positional Collision: Signal {} exceeds remaining context space.", i);
                    return Err(anyhow::anyhow!(
                        "CluaizeSignal: Context Overflow at Signal {}",
                        i
                    ));
                }

                println!(
                    "🧵 [Llama-Engine] Stitching Signal {} ({} tokens) at offset {}.",
                    i, token_count, current_offset
                );

                for layer_idx in 0..max_layers as i32 {
                    // Note: lucebox.stitch_kv_layer will eventually need to take the offset.
                    // For Phase 1 of Mission 10, we assume sequential allocation in the kernel.
                    if let Err(e) = lucebox.stitch_kv_layer(layer_idx, &*signal.raw_data) {
                        tracing::error!(
                            "❌ [Llama-Engine] Stitching failed at Signal {}, Layer {}: {}",
                            i,
                            layer_idx,
                            e
                        );
                        return Err(e);
                    }
                }

                current_offset += token_count;
            }

            println!("✅ [Llama-Engine] Multi-Soul Fusion: {} signals stitched successfully. [Total Context: {}/{}]", 
                signals.len(), current_offset, max_ctx);
            Ok(())
        } else {
            tracing::warn!("⚠️ [Llama-Engine] Injection skipped: No Lucebox accelerator attached.");
            Ok(())
        }
    }

    /// 🚀 Booster Sync: Applies hardware-level optimization flags (TurboQuant, KV-Cache, etc.)
    fn apply_booster(
        &mut self,
        control: &cluaize_shared::hardware::schema::booster::BoosterControl,
    ) -> Result<()> {
        tracing::info!("🚀 [Llama-Engine] Applying Booster: Autonomous Performance Sync");

        // 🔄 Sync local booster state from system
        self.booster = crate::config::BoosterConfig::load_from_system();

        // 🌊 Trigger Elastic Resize (VRAM Sovereignty)
        if let Some(native) = &mut self.native {
            let mut ctx_params = self.booster.to_context_params();

            // Recalculate context window through Governor using the injected control truth
            let new_ctx = cluaize_shared::hardware::governor::HardwareGovernor::negotiate_vram_envelope_with_booster(&self.context.dna, control);
            ctx_params.n_ctx = new_ctx as u32;

            // Sync settings dynamically
            native.kv_cache_quantization_mode = match control.kv_cache_quantization {
                cluaize_shared::hardware::schema::booster::KvCacheQuantization::Kv8 => 1,
                cluaize_shared::hardware::schema::booster::KvCacheQuantization::Kv4 => 2,
                _ => 0,
            };
            native.context_shifting_mode = match control.context_shifting {
                cluaize_shared::hardware::schema::booster::ContextShiftingMode::Off => 0,
                cluaize_shared::hardware::schema::booster::ContextShiftingMode::Minimal => 1,
                cluaize_shared::hardware::schema::booster::ContextShiftingMode::Standard
                | cluaize_shared::hardware::schema::booster::ContextShiftingMode::Auto => 2,
                cluaize_shared::hardware::schema::booster::ContextShiftingMode::Aggressive => 3,
                cluaize_shared::hardware::schema::booster::ContextShiftingMode::Extreme => 4,
            };

            native.resize_context(ctx_params)?;
            tracing::info!(
                "🌊 [Llama-Engine] Elastic Resize Success: Context now {} tokens.",
                new_ctx
            );
        }

        Ok(())
    }

    /// 🌊 Liquid Execution: Activates adaptive context density.
    fn set_liquid_mode(&mut self, enabled: bool) -> Result<()> {
        tracing::info!("🌊 [Llama-Engine] Liquid Mode set to: {}", enabled);
        Ok(())
    }

    /// 💾 Native Memory Dump: Extracts the actual KV cache buffer to a binary file.
    fn dump_kv_cache(&mut self, path: &str) -> Result<()> {
        if let Some(ref native) = self.native {
            if !native.ctx_ptr.is_null() {
                let c_path = std::ffi::CString::new(path)?;
                let bytes_written = unsafe {
                    if !self.last_prefilled_tokens.is_empty() {
                        crate::ffi::llama_cpp::llama_state_seq_save_file(
                            native.ctx_ptr,
                            c_path.as_ptr(),
                            0, // seq_id
                            self.last_prefilled_tokens.as_ptr(),
                            self.last_prefilled_tokens.len(),
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
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("llama_state_seq_save_file failed"))
                }
            } else {
                Err(anyhow::anyhow!("Context pointer is null"))
            }
        } else {
            Err(anyhow::anyhow!("Native backend not initialized"))
        }
    }

    /// 💾 Load KV Cache from a binary file.
    fn load_kv_cache(&mut self, path: &str) -> Result<()> {
        if let Some(ref native) = self.native {
            if !native.ctx_ptr.is_null() {
                let c_path = std::ffi::CString::new(path)?;
                let mut tokens = vec![0i32; 16384]; // Temp tokens vector
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
                    self.last_prefilled_tokens = tokens[..n_tokens_out].to_vec();
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("llama_state_seq_load_file failed"))
                }
            } else {
                Err(anyhow::anyhow!("Context pointer is null"))
            }
        } else {
            Err(anyhow::anyhow!("Native backend not initialized"))
        }
    }
}

// ─── Sovereign FFI Gateway ──────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn cluaize_kernel_init() -> *const std::os::raw::c_char {
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
    }
    tracing::info!("🧬 [Llama.cpp-Kernel] Sovereign Handshake & Backend Initialized.");
    "cluaize-llama.cpp-active\0".as_ptr() as *const std::os::raw::c_char
}

#[used]
static _FORCE_KEEP_INIT: extern "C" fn() -> *const std::os::raw::c_char = cluaize_kernel_init;

#[no_mangle]
pub extern "C" fn cluaize_kernel_instantiate(
    path_ptr: *const std::os::raw::c_char,
    booster_ptr: *const cluaize_shared::hardware::schema::booster::CluaizeBoosterContext,
) -> *mut RuntimeB {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let path_str = unsafe { std::ffi::CStr::from_ptr(path_ptr) }
            .to_string_lossy()
            .into_owned();

        let model_path = std::path::Path::new(&path_str);
        let model_dir = model_path.parent().unwrap_or(model_path);

        tracing::info!(
            "🧬 [Llama-Lib] Initiating Sovereign DNA Handshake for: {:?}",
            model_dir
        );
        let mut dna = cluaize_shared::metadata::dna::StructuralDNA::load(
            &model_dir.join("structural_dna.json"),
        )
        .unwrap_or_else(|_| {
            println!("⚠️ [Llama-Lib] DNA Manifest missing. Creating transient skeleton...");
            cluaize_shared::metadata::dna::StructuralDNA::default()
        });

        // ALWAYS perform real-time discovery to sync with LIVE hardware state
        eprintln!("📂 [Llama-Lib] Discovering real-time truth...");
        if let Err(e) = dna.discover_from_path(model_dir) {
            eprintln!(
                "⚠️ [Llama-Lib] DNA Discovery Failed: {}. Using best-effort constraints.",
                e
            );
        }
        eprintln!(
            "✅ [Llama-Lib] DNA Discovery Complete. Negotiated Context: {:?}",
            dna.max_context_length
        );
        eprintln!("📊 [Llama-Lib] Weights Size: {:.2}GB", dna.weights_size_gb);

        let context = CluaizeContext::boot(dna, cluaize_shared::TemplateManager::default());
        let mut engine = Box::new(RuntimeB::new(&path_str, context));

        // Inject Booster Configuration from Caller
        if !booster_ptr.is_null() {
            let booster_ctx = unsafe { *booster_ptr };
            println!(
                "🚀 [Llama.cpp-Kernel] Received CluaizeBoosterContext via FFI: {:?}",
                booster_ctx
            );
            tracing::info!(
                "🚀 [Llama.cpp-Kernel] Received CluaizeBoosterContext via FFI: {:?}",
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
                cluaize_shared::hardware::governor::HardwareGovernor::load_booster_settings()
            {
                let _ = engine.apply_booster(&booster);
            }
        }

        // 🧬 Trigger Native Load immediately on instantiation
        if let Err(e) = engine.load_native() {
            eprintln!("❌ [Llama.cpp-Kernel] Native Load Failed: {}", e);
            tracing::error!("❌ [Llama.cpp-Kernel] Native Load Failed: {}", e);
            return std::ptr::null_mut();
        }

        Box::into_raw(engine)
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            tracing::error!(
                "🚨 [FFI-Panic] Caught panic in cluaize_kernel_instantiate! Preventing OS crash."
            );
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn cluaize_kernel_generate_stream(
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
                tracing::error!("❌ [Llama-Engine] Generation failed: {}", e);
                -2
            }
        }
    }));

    match result {
        Ok(res) => res,
        Err(_) => {
            tracing::error!("🚨 [FFI-Panic] Caught panic in cluaize_kernel_generate_stream!");
            -3
        }
    }
}

#[no_mangle]
pub extern "C" fn cluaize_kernel_free(engine_ptr: *mut RuntimeB) {
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
        tracing::error!("🚨 [FFI-Panic] Caught panic in cluaize_kernel_free!");
    }
}

#[no_mangle]
pub extern "C" fn cluaize_kernel_set_skip_ptr(ptr: *const std::sync::atomic::AtomicBool) {
    unsafe {
        crate::native::stream::SKIP_PTR = ptr;
    }
}

#[no_mangle]
pub extern "C" fn cluaize_kernel_dump_kv_cache(
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
pub extern "C" fn cluaize_kernel_load_kv_cache(
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
                let mut tokens = vec![0i32; 16384]; // Temp tokens vector
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
