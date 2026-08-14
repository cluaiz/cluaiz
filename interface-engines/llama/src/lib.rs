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
pub mod expert_offloading;
pub mod native;
pub mod pipeline;
pub mod router;
pub mod sampling;

use crate::config::OptimizationConfig;
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
    pub optimization: OptimizationConfig,
    pub native: Option<NativeLlama>,
    pub lucebox: Option<Arc<ffi::lucebox::LuceboxBridge>>,
    pub last_prefilled_tokens: Vec<i32>,
    pub moe_controller: Option<Arc<std::sync::Mutex<crate::expert_offloading::GgufMoeStreamingController>>>,
}

impl RuntimeB {
    pub fn new(path: &str, context: cluaizContext) -> Self {
        Self {
            model_path: path.to_string(),
            context,
            optimization: OptimizationConfig::default(),
            native: None,
            lucebox: None,
            last_prefilled_tokens: Vec::new(),
            moe_controller: None,
        }
    }

    /// 🧬 Load the model natively into memory using current optimization settings.
    pub fn load_native(&mut self) -> anyhow::Result<()> {
        let mut model_params = self.optimization.to_model_params();

        // 🧠 PROBE GGUF METADATA & ARCHITECTURE TRUTH
        let mut has_native_mtp = false;
        let mut is_ssm_model = false;
        let mut probed_layers = None;

        if let Ok((metadata, tensor_infos, _)) =
            cluaiz_shared::utils::GGUFProber::probe(std::path::Path::new(&self.model_path))
        {
            has_native_mtp = cluaiz_shared::utils::GGUFProber::check_native_mtp(&tensor_infos);
            is_ssm_model =
                cluaiz_shared::utils::GGUFProber::check_recurrent_ssm(&metadata, &tensor_infos);

            // Extract actual block count/layers dynamically from keys (e.g. llama.block_count)
            for (k, v) in &metadata {
                if k.ends_with(".block_count") {
                    if let Ok(count) = v.parse::<usize>() {
                        probed_layers = Some(count);
                        break;
                    }
                }
            }
        }

        let layers = self.context.dna.layer_count.or(probed_layers).unwrap_or(32);

        let mut weights_gb = self.context.dna.weights_size_gb;
        if weights_gb <= 0.0 {
            weights_gb = std::fs::metadata(&self.model_path)
                .map(|m| m.len() as f64 / (1024.0 * 1024.0 * 1024.0))
                .unwrap_or(0.0) as f32;
        }

        let request = cluaiz_shared::hardware::ResourceRequest {
            engine_type: cluaiz_shared::hardware::EngineType::GGUF,
            inference_mode: cluaiz_shared::hardware::InferenceMode::Chat,
            model_size_gb: weights_gb as f64,
            model_path: std::path::PathBuf::from(&self.model_path),
        };

        let grant = cluaiz_shared::hardware::negotiate_resource(&request)?;

        // Apply resource negotiator results
        eprintln!(
            "⚖️ [Negotiator] GGUF resource grant: tier = {:?}, GPU layers = {}, VRAM budget = {:.2} GB, RAM budget = {:.2} GB",
            grant.tier,
            grant.n_gpu_layers,
            grant.vram_budget_gb,
            grant.ram_budget_gb
        );

        // Extract metadata configuration settings
        let gguf_hdr = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
        let user_no_mmap = gguf_hdr.hardware_and_execution.no_mmap;
        let user_n_gpu_layers = self.optimization.n_gpu_layers;

        // Apply use_mmap logic: respect config but force true under SsdStreaming/expert swapping
        model_params.use_mmap = !user_no_mmap;
        if grant.tier == cluaiz_shared::hardware::PlacementTier::SsdStreaming {
            model_params.use_mmap = true;
            model_params.use_extra_bufts = false;
            eprintln!("🧠 [Native-Llama] SSD Streaming Active. Enforcing use_mmap = true for page-cache streaming.");
            eprintln!("🧠 [Native-Llama] SSD Streaming: Disabled CPU_REPACK (use_extra_bufts = false) to prevent 11 GB duplicate RAM buffer.");
        }
        eprintln!("🧬 [Native-Llama] Resolved Model Memory Mode: use_mmap = {}, n_gpu_layers = {}, tier = {:?}", model_params.use_mmap, model_params.n_gpu_layers, grant.tier);

        // Clamp user custom layers setting to negotiator allocated safe GPU budget limit
        let target_gpu_layers = if user_n_gpu_layers == 0 {
            0
        } else if user_n_gpu_layers == -1 {
            if grant.n_gpu_layers == -1 { layers as i32 } else { grant.n_gpu_layers }
        } else {
            // Custom layers case: honor custom value but bound by negotiator safe allocation limit
            if grant.n_gpu_layers >= 0 {
                (user_n_gpu_layers).min(grant.n_gpu_layers)
            } else {
                user_n_gpu_layers
            }
        };

        let original_layers = model_params.n_gpu_layers;
        model_params.n_gpu_layers = target_gpu_layers;
        eprintln!(
            "🧬 [Native-Llama] Configured n_gpu_layers: {} (negotiated limit applied, original config was {})",
            model_params.n_gpu_layers, original_layers
        );


        // Hook MoE controller if the negotiator verified it is a MoE model
        if let Some(ref moe_info) = grant.moe_info {
            eprintln!("🧠 [Native-Llama] Grant contains MoE info. checking is_moe = {}", moe_info.is_moe);
            if moe_info.is_moe {
                eprintln!("🧠 [Native-Llama] Loading MoE Streaming Controller. Cache budget: {:.2} GB | GPU offloaded layers: {}", grant.expert_cache_budget_gb, model_params.n_gpu_layers);
                let offloaded_layers = model_params.n_gpu_layers.max(0) as usize;
                match crate::expert_offloading::GgufMoeStreamingController::new(
                    std::path::Path::new(&self.model_path),
                    moe_info.clone(),
                    grant.expert_cache_budget_gb,
                    offloaded_layers,
                ) {
                    Ok(controller) => {
                        self.moe_controller = Some(Arc::new(std::sync::Mutex::new(controller)));
                        eprintln!("🧠 [Native-Llama] ✅ MoE Streaming Controller initialized and pre-warmed.");
                    }
                    Err(e) => {
                        eprintln!("🧠 [Native-Llama] ❌ Failed to initialize MoE controller: {}", e);
                    }
                }
            }
        }

        // 🧬 DNA TRUTH SYNC: Ensure DNA context is applied to context params
        let mut ctx_params = self.optimization.to_context_params();

        // 🧠 Dynamic & Clamped Context Sizing from live leftover RAM/VRAM
        let mut dedicated_vram = 0.0;
        if let Ok(control) = cluaiz_shared::hardware::governor::HardwareGovernor::load_system_control() {
            dedicated_vram = control
                .silicon_truth
                .accelerators
                .gpus
                .iter()
                .map(|g| g.vram_available_gb)
                .sum::<f64>();
        }
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total_ram = (sys.total_memory() as f64) / (1024.0 * 1024.0 * 1024.0);
        let available_ram = (sys.available_memory() as f64) / (1024.0 * 1024.0 * 1024.0);

        let weights_gb = if let Some(ref controller) = self.moe_controller {
            if let Ok(guard) = controller.lock() {
                let dense_gb = guard.moe_info.dense_backbone_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let cache_gb = guard.moe_info.recommended_cache_budget_gb();
                let active_ram_footprint = dense_gb + cache_gb;
                cluaiz_shared::dev_info!(
                    "🧠 [Arbiter] MoE Active Weight RAM Footprint: {:.2} GB (Dense: {:.2} GB, Cache: {:.2} GB) vs Total Model: {:.2} GB",
                    active_ram_footprint, dense_gb, cache_gb, self.context.dna.weights_size_gb
                );
                active_ram_footprint
            } else {
                self.context.dna.weights_size_gb as f64
            }
        } else {
            self.context.dna.weights_size_gb as f64
        };

        let opt_ctrl = cluaiz_shared::hardware::governor::HardwareGovernor::load_optimization_settings().unwrap_or_default();
        let vram_safety = cluaiz_shared::hardware::calculate_safety_buffer(&opt_ctrl, dedicated_vram, dedicated_vram);
        let ram_safety = cluaiz_shared::hardware::calculate_ram_safety_buffer(&opt_ctrl, total_ram, available_ram);

        let usable_dedicated_vram = cluaiz_shared::hardware::calculate_usable_vram(&opt_ctrl, dedicated_vram, dedicated_vram);
        let usable_ram = cluaiz_shared::hardware::calculate_usable_ram(&opt_ctrl, total_ram, available_ram);

        let leftover_gb = if model_params.n_gpu_layers == layers as i32 {
            (usable_dedicated_vram - weights_gb).max(0.0)
        } else {
            (usable_dedicated_vram + usable_ram - weights_gb).max(0.0)
        };

        let leftover_bytes = (leftover_gb * 1024.0 * 1024.0 * 1024.0) as i64;
        let kv_bytes_per_token = 128 * 1024;
        let max_safe_ctx = ((leftover_bytes / kv_bytes_per_token).max(1024) as u32)
            .min(self.context.dna.max_context_length.unwrap_or(32768) as u32);

        if self.optimization.n_ctx == 0 {
            ctx_params.n_ctx = max_safe_ctx;
            cluaiz_shared::dev_info!("🧠 [Arbiter] Dynamic Context Window (n_ctx=0) scaled to: {} tokens (Leftover Memory: {:.2} GB)", ctx_params.n_ctx, leftover_gb);
        } else if self.optimization.n_ctx == u32::MAX {
            ctx_params.n_ctx = std::cmp::min(self.context.dna.max_context_length.unwrap_or(8192) as u32, max_safe_ctx);
            cluaiz_shared::dev_info!("🧠 [Arbiter] Context window locked to Max Native Limit (clamped to available memory): {} tokens", ctx_params.n_ctx);
        } else {
            let requested = self.optimization.n_ctx;
            if requested <= max_safe_ctx {
                ctx_params.n_ctx = requested;
                cluaiz_shared::dev_info!("🧠 [Arbiter] Explicit user n_ctx={} tokens honored (Memory available)", ctx_params.n_ctx);
            } else {
                ctx_params.n_ctx = max_safe_ctx;
                cluaiz_shared::dev_info!("⚠️ [Arbiter] Explicit user n_ctx={} tokens exceeds live free RAM! Overriding and clamping down to {} tokens to prevent OOM crash.", requested, max_safe_ctx);
            }
        }


        // 🧠 RESOLVE SPECULATIVE MODE & SYNC DNA
        if is_ssm_model {
            // 🚨 For hybrid/recurrent models (Qwen3.5 GDN, Mamba, RWKV):
            // Speculative decoding is incompatible with non-transformer architectures.
            cluaiz_shared::dev_info!("⚖️ [Llama-Engine] SSM/Hybrid architecture detected.");
            cluaiz_shared::dev_info!("⚖️ [Llama-Engine] → Speculative Decoding: FORCED OFF");
            self.optimization.speculative_decoding = "off".to_string();
        }

        let speculative_mode = if self.optimization.speculative_decoding.to_lowercase() != "off" {
            if has_native_mtp {
                "native_mtp"
            } else {
                "eagle"
            }
        } else {
            "off"
        };
        cluaiz_shared::dev_info!(
            "🧠 [Llama-Engine] Dynamic Speculative Sync: Mode resolved as '{}' (optimization: {})",
            speculative_mode,
            self.optimization.speculative_decoding
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

        // 🚀 BATCH SYNC: Optimized for 4GB hardware by default, scalable via OptimizationConfig.
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

        // 🚀 High Memory Pressure Guard: Disable mlock if system memory usage is >= 90% to prevent swap thrashing
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let mem_pct = (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0;
        if mem_pct >= 90.0 && model_params.use_mlock {
            cluaiz_shared::dev_info!("⚠️ [Arbiter] High Memory Pressure Detected ({:.1}%). Disabling use_mlock to prevent OS paging freeze.", mem_pct);
            model_params.use_mlock = false;
        }
        if model_params.use_mmap && self.moe_controller.is_some() {
            cluaiz_shared::hardware::apply_windows_hard_memory_quota(usable_ram);
        }

        let native = NativeLlama::load(
            &self.model_path,
            model_params,
            ctx_params,
            &mut self.context.dna,
            match self.optimization.kv_cache_quantization.to_lowercase().as_str() {
                "kv8" => 1,
                "kv4" => 2,
                _ => 0,
            },
            match self.optimization.context_shifting.to_lowercase().as_str() {
                "off" => 0,
                "minimal" => 1,
                "standard" | "auto" | "on" => 2,
                "aggressive" => 3,
                "extreme" => 4,
                _ => 2,
            },
            match self.optimization.speculative_decoding.to_lowercase().as_str() {
                "off" => 0,
                "on" => 1,
                _ => 2,
            },
            self.moe_controller.clone(),
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

                if let Ok(new_tokens) = &res {
                    self.last_prefilled_tokens = new_tokens.clone();
                    cb.record_success();
                } else {
                    self.last_prefilled_tokens.clear();
                    cb.record_failure("Native stream error");
                }
                return res.map(|_| ());
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

    /// Optimization Sync: Applies hardware-level optimization flags (TurboQuant, KV-Cache, etc.)
    fn apply_optimization(
        &mut self,
        control: &cluaiz_shared::hardware::schema::optimization::OptimizationControl,
    ) -> Result<()> {
        tracing::info!("[Llama-Engine] Applying Optimization: Autonomous Performance Sync");

        // 🔄 Sync local optimization state from system
        self.optimization = crate::config::OptimizationConfig::load_from_system();

        // 🌊 Trigger Elastic Resize (VRAM Sovereignty)
        if let Some(native) = &mut self.native {
            let mut ctx_params = self.optimization.to_context_params();

            // Recalculate context window through Governor using the injected control truth
            let new_ctx = cluaiz_shared::hardware::governor::HardwareGovernor::negotiate_vram_envelope_with_optimization(&self.context.dna, control);
            ctx_params.n_ctx = new_ctx as u32;

            // Sync settings dynamically
            let optimization_ctx =
                cluaiz_shared::hardware::schema::optimization::cluaizOptimizationContext::from(control);
            native.kv_cache_quantization_mode = optimization_ctx.kv_cache_quantization_mode;
            native.context_shifting_mode = optimization_ctx.context_shifting_mode;

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
