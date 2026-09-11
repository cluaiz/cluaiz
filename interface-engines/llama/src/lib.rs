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
pub mod dma_streamer;
pub mod expert_offloading;
pub mod ffi;
pub mod ffi_exports;
pub mod hybrid;
pub mod loader;
pub mod native;
pub mod pipeline;
pub mod router;
pub mod sampling;
pub use dma_streamer::{
    CudaDeviceScratchBuffer, CudaDmaStreamer, CudaPinnedHostBuffer, DeviceScratchBuffer,
    DmaStreamer, PinnedHostBuffer, SiliconDeviceScratchBuffer, SiliconDmaStreamer,
    SiliconPinnedHostBuffer,
};

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
    pub moe_controller:
        Option<Arc<std::sync::Mutex<crate::expert_offloading::GgufMoeStreamingController>>>,
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

        // 🎯 Single Source of Truth: Query model_registry.json directly if present
        let reg_path = cluaiz_shared::environment::EnvironmentManager::current().model_registry_json_path();
        if reg_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&reg_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(installed) = val.get("installed_models").and_then(|m| m.as_object()) {
                        let path_norm = self.model_path.to_lowercase().replace('\\', "/");
                        for (_id, entry) in installed {
                            let local_dir = entry.get("local_dir").and_then(|d| d.as_str()).unwrap_or("").to_lowercase().replace('\\', "/");
                            let primary_file = entry.get("files")
                                .and_then(|f| f.as_array())
                                .and_then(|arr| arr.iter().find(|f| f.get("is_primary").and_then(|p| p.as_bool()).unwrap_or(false)))
                                .and_then(|f| f.get("name").and_then(|n| n.as_str()))
                                .unwrap_or("")
                                .to_lowercase();
                            let matches = (!local_dir.is_empty() && path_norm.contains(&local_dir))
                                || (!primary_file.is_empty() && path_norm.contains(&primary_file));
                            if matches {
                                if let Some(meta) = entry.get("metadata") {
                                    if let Some(arch) = meta.get("architecture").and_then(|a| a.as_str()) {
                                        self.context.dna.model_identity = arch.to_string();
                                    }
                                    if let Some(lc) = meta.get("layer_count").and_then(|l| l.as_u64()) {
                                        probed_layers = Some(lc as usize);
                                    }
                                    if let Some(ssm) = meta.get("is_ssm_model").and_then(|s| s.as_bool()) {
                                        is_ssm_model = ssm;
                                    }
                                    if let Some(mtp) = meta.get("has_native_mtp").and_then(|m| m.as_bool()) {
                                        has_native_mtp = mtp;
                                    }
                                }
                                break;
                            }
                        }
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
        model_params.set_mmap(!user_no_mmap);
        if grant.tier == cluaiz_shared::hardware::PlacementTier::SsdStreaming {
            model_params.set_mmap(true);
            model_params.use_extra_bufts = true;
            eprintln!("🧠 [Native-Llama] SSD Streaming Active. Enforcing use_mmap = true for page-cache streaming.");
            eprintln!("🧠 [Native-Llama] SSD Streaming: Disabled CPU_REPACK (use_extra_bufts = false) to prevent 11 GB duplicate RAM buffer.");
        }
        eprintln!("🧬 [Native-Llama] Resolved Model Memory Mode: load_mode = {}, n_gpu_layers = {}, tier = {:?}", model_params.load_mode, model_params.n_gpu_layers, grant.tier);

        // Clamp user custom layers setting to negotiator allocated safe GPU budget limit
        let target_gpu_layers = if user_n_gpu_layers == 0 {
            0
        } else if user_n_gpu_layers == -1 {
            if grant.n_gpu_layers == -1 {
                -1
            } else {
                grant.n_gpu_layers
            }
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
            eprintln!(
                "🧠 [Native-Llama] Grant contains MoE info. checking is_moe = {}",
                moe_info.is_moe
            );
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
                        eprintln!(
                            "🧠 [Native-Llama] ❌ Failed to initialize MoE controller: {}",
                            e
                        );
                    }
                }
            }
        }

        // 🧬 DNA TRUTH SYNC: Ensure DNA context is applied to context params
        let mut ctx_params = self.optimization.to_context_params();

        // 🧠 Dynamic Context Window (Single Source of Truth from Unified Resource Negotiator, min 2048)
        ctx_params.n_ctx = grant.target_ctx_tokens as u32;
        cluaiz_shared::dev_info!(
            "🧠 [Arbiter] Dynamic Context Window set to {} tokens (Single Source of Truth, min 2048)",
            ctx_params.n_ctx
        );

        ctx_params.swa_full = 0; // Enforce safe SWA cache sizing

        // 🛡️ Dynamic Flash Attention Policy: Flash Attention is strictly a pure-GPU kernel.
        // If the Negotiator placed the model in Hybrid, CPU, or SSD Streaming, disable Flash Attention
        // to prevent cross-device numerical divergence (NaNs) in split attention graphs.
        if grant.tier != cluaiz_shared::hardware::PlacementTier::GpuOnly
            || (model_params.n_gpu_layers >= 0 && model_params.n_gpu_layers < layers as i32)
        {
            cluaiz_shared::dev_info!(
                "⚖️ [Arbiter] Placement tier is {:?} or layers split across GPU/CPU (n_gpu_layers = {}). Flash Attention disabled for cross-device stability.",
                grant.tier,
                model_params.n_gpu_layers
            );
            ctx_params.flash_attn_type = 0;
        }

        // 🛡️ Dynamic Architecture Capability Guards (Zero Hardcoded Model Names)
        let is_recurrent_ssm = is_ssm_model || self.context.dna.signature.is_ssm;
        if !self.context.dna.supports_flash_attention() || is_recurrent_ssm {
            cluaiz_shared::dev_info!("🛡️ [Architecture Guard] Non-standard attention geometry detected: Disabling Flash Attention to prevent numerical divergence.");
            ctx_params.flash_attn_type = 0;
        }
        if self.context.dna.requires_fp16_kv() || is_recurrent_ssm || (grant.tier != cluaiz_shared::hardware::PlacementTier::GpuOnly && ctx_params.flash_attn_type == 0) {
            cluaiz_shared::dev_info!("🛡️ [Architecture Guard] Enforcing F16 KV-cache for mathematical stability across splits.");
            ctx_params.type_k = 1; // GGML_TYPE_F16
            ctx_params.type_v = 1; // GGML_TYPE_F16
        }

        // 🧠 RESOLVE SPECULATIVE MODE & SYNC DNA
        if is_recurrent_ssm {
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
        } else if self.moe_controller.is_some() || grant.vram_budget_gb <= 6.0 {
            // 🛡️ MoE / 4GB-6GB VRAM Stream Decoding: Cap batch to 512 / ubatch to 128
            // This cuts GGML compute graph workspace from ~880 MB to ~150 MB,
            // preventing CUDA OOM and graph split allocation failures.
            ctx_params.n_batch = 512;
            ctx_params.n_ubatch = 128;
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
        if mem_pct >= 90.0 && model_params.is_mlock() {
            cluaiz_shared::dev_info!("⚠️ [Arbiter] High Memory Pressure Detected ({:.1}%). Disabling use_mlock to prevent OS paging freeze.", mem_pct);
            model_params.set_mlock(false);
        }
        if model_params.is_mmap() && self.moe_controller.is_some() {
            cluaiz_shared::hardware::apply_windows_hard_memory_quota(grant.ram_budget_gb);
        }

        let native = NativeLlama::load(
            &self.model_path,
            model_params,
            ctx_params,
            &mut self.context.dna,
            match self
                .optimization
                .kv_cache_quantization
                .to_lowercase()
                .as_str()
            {
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
            match self
                .optimization
                .speculative_decoding
                .to_lowercase()
                .as_str()
            {
                "off" => 0,
                "on" => 1,
                _ => 2,
            },
            self.moe_controller.clone(),
        )?;
        self.native = Some(native);
        tracing::info!("✅ [Llama-Engine] Native Model Loaded & Optimized.");

        // 🚀 DEFERRED DMA INIT: Now that model is loaded and VRAM is occupied,
        // CudaDmaStreamer will see real post-load free VRAM (~250 MB) and allocate
        // appropriately sized pinned host buffers (~204 MB instead of ~2.70 GB).
        if let Some(ref controller) = self.moe_controller {
            if let Ok(mut guard) = controller.lock() {
                guard.init_dma_streamer();
            }
        }

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
        let max_ctx = (self.optimization.n_ctx as usize).max(2048);
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
                cluaiz_shared::hardware::schema::optimization::cluaizOptimizationContext::from(
                    control,
                );
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
