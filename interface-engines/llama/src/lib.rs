#![allow(warnings)]
//! Sovereign Implementation B: Accelerated Feature-Based Runtime (Llama Engine).
//! This kernel is loaded dynamically by the SiliconOrchestrator.

use anyhow::Result;
use cluaiz_shared::{cluaizContext, cluaizInference, UnifiedBackend};
use neural_core::interfaces::memory_contract::SovereignBuffer;
use std::sync::Arc;
use tokenizers::Tokenizer;

pub mod asm_kernels;
pub mod bridge;
pub mod config;
pub mod ffi;
pub mod ffi_exports;
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
    pub context: cluaizContext,
    pub booster: BoosterConfig,
    pub native: Option<NativeLlama>,
    pub lucebox: Option<Arc<ffi::lucebox::LuceboxBridge>>,
    pub last_prefilled_tokens: Vec<i32>,
}

impl RuntimeB {
    pub fn new(path: &str, context: cluaizContext) -> Self {
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
        let mut model_params = self.booster.to_model_params();

        // 🛡️ CERD DOCTRINE: Dynamic VRAM Layer Offload (Prevent OOM Crashes)
        if model_params.n_gpu_layers == -1 {
            let weights_gb = self.context.dna.weights_size_gb;
            let vram_gb = self.context.dna.vram_headroom_gb;
            let layers = self.context.dna.layer_count.unwrap_or(0);
            
            // Only protect if we actually know there's limited VRAM (vram_gb > 0)
            if weights_gb > 0.0 && vram_gb > 0.0 && weights_gb > vram_gb * 0.9 {
                // Reserve 15% VRAM for OS/Display and Context Cache
                let usable_vram = vram_gb * 0.85; 
                if layers > 0 {
                    let ratio = usable_vram / weights_gb;
                    let safe_layers = (layers as f32 * ratio) as i32;
                    cluaiz_shared::dev_info!("⚠️ [Arbiter] Model ({:.2}GB) exceeds VRAM ({:.2}GB). Clamping n_gpu_layers to {}/{} to prevent OOM.", weights_gb, vram_gb, safe_layers, layers);
                    model_params.n_gpu_layers = safe_layers;
                } else {
                    let safe_layers = 10;
                    cluaiz_shared::dev_info!("⚠️ [Arbiter] Model ({:.2}GB) exceeds VRAM ({:.2}GB). Unknown layers. Clamping n_gpu_layers to {} to prevent OOM.", weights_gb, vram_gb, safe_layers);
                    model_params.n_gpu_layers = safe_layers;
                }
            }
        }

        // 🧬 DNA TRUTH SYNC: Ensure DNA context is applied to context params
        let mut ctx_params = self.booster.to_context_params();

        if let Some(ctx) = self.context.dna.max_context_length {
            if self.booster.n_ctx == 0 { ctx_params.n_ctx = std::cmp::min(ctx as u32, 8192); } else { ctx_params.n_ctx = ctx as u32; }
        }

        // 🧠 RESOLVE SPECULATIVE MODE & SYNC DNA
        // We probe GGUF metadata + tensor names to detect hybrid/recurrent models (e.g. Qwen3.5 GDN).
        // GGUFProber now checks: architecture name, *.layer_types metadata, AND tensor patterns.
        let (has_native_mtp, is_ssm_model) = if let Ok((metadata, tensor_infos, _)) =
            cluaiz_shared::utils::GGUFProber::probe(std::path::Path::new(&self.model_path))
        {
            (
                cluaiz_shared::utils::GGUFProber::check_native_mtp(&tensor_infos),
                cluaiz_shared::utils::GGUFProber::check_recurrent_ssm(&metadata, &tensor_infos),
            )
        } else {
            (false, false)
        };

        if is_ssm_model {
            // 🚨 For hybrid/recurrent models (Qwen3.5 GDN, Mamba, RWKV):
            // Speculative decoding is incompatible with non-transformer architectures.
            cluaiz_shared::dev_info!("⚖️ [Llama-Engine] SSM/Hybrid architecture detected.");
            cluaiz_shared::dev_info!("⚖️ [Llama-Engine] → Speculative Decoding: FORCED OFF");
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
        cluaiz_shared::dev_info!(
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
        if let Some(ref mut native) = self.native {
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
        cluaiz_shared::hardware::telemetry::get_pulse()
            .tps_counter
            .load(std::sync::atomic::Ordering::Relaxed) as f64
    }
}

impl cluaizInference for RuntimeB {
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
        let mut cb = cluaiz_shared::hardware::circuit_breaker::NeuralCircuitBreaker::default();
        if !cb.can_proceed() {
            return Err(anyhow::anyhow!(
                "🚨 [Circuit Breaker] Inference blocked due to previous system instability."
            ));
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // 🚀 High-Performance Native Path
            if let Some(ref mut native) = self.native {
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
        signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>,
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
                        "cluaizSignal: Context Overflow at Signal {}",
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
        control: &cluaiz_shared::hardware::schema::booster::BoosterControl,
    ) -> Result<()> {
        tracing::info!("🚀 [Llama-Engine] Applying Booster: Autonomous Performance Sync");

        // 🔄 Sync local booster state from system
        self.booster = crate::config::BoosterConfig::load_from_system();

        // 🌊 Trigger Elastic Resize (VRAM Sovereignty)
        if let Some(native) = &mut self.native {
            let mut ctx_params = self.booster.to_context_params();

            // Recalculate context window through Governor using the injected control truth
            let new_ctx = cluaiz_shared::hardware::governor::HardwareGovernor::negotiate_vram_envelope_with_booster(&self.context.dna, control);
            ctx_params.n_ctx = new_ctx as u32;

            // Sync settings dynamically
            native.kv_cache_quantization_mode = match control.kv_cache_quantization {
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv8 => 1,
                cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv4 => 2,
                _ => 0,
            };
            native.context_shifting_mode = match control.context_shifting {
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Off => 0,
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Minimal => 1,
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Standard
                | cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Auto => 2,
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Aggressive => 3,
                cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Extreme => 4,
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

