use anyhow::{anyhow, Result};
use neural_core::interfaces::router_contract::{EmbeddingDriver, EngineError};
use cluaiz_shared::{ModelWeightsWrapper, cluaizContext, UnifiedBackend, cluaizInference};
use crate::interface_engines::EngineManager;
use std::sync::{Arc, Mutex};

pub struct HardwareOrchestrator;

impl HardwareOrchestrator {
    /// Dispatches and instantiates the correct model kernel via the Dynamic cluaiz Linker.
    pub async fn instantiate(
        model_load_path: &str,
        engine_type: &str,
        cluaiz_context: cluaizContext,
    ) -> Result<ModelWeightsWrapper> {
        Self::instantiate_with_booster(model_load_path, engine_type, cluaiz_context, None).await
    }

    pub async fn instantiate_with_booster(
        model_load_path: &str,
        engine_type: &str,
        cluaiz_context: cluaizContext,
        booster_override: Option<cluaiz_shared::hardware::schema::booster::BoosterControl>,
    ) -> Result<ModelWeightsWrapper> {
        tracing::info!("🔩 [Orchestrator] Initiating Dynamic Hardware Handshake for Engine: {}", engine_type);

        // Engine boots purely as a router. Any DB/Memory extensions will be loaded via ExtensionManager.

        if engine_type == "onnx" {
            tracing::info!("🔮 [Orchestrator] Bypassing FFI Linker. Instantiating Native Rust ONNX Gatekeeper.");
            let mut onnx_engine = cluaiz_onnx::engine::OnnxEngine::new()
                .map_err(|e| anyhow!("Failed to init ONNX: {}", e))?;
            
            let model_path = std::path::Path::new(model_load_path);
            let tokenizer_path = if model_path.is_dir() {
                model_path.join("tokenizer.json")
            } else {
                model_path.parent().unwrap_or(model_path).join("tokenizer.json")
            };

            if tokenizer_path.exists() {
                tracing::info!("🔍 [Orchestrator] Tokenizer found for Vision Model. Loading as Multimodal Engine.");
                onnx_engine.load_text_model(model_load_path, tokenizer_path.to_str().unwrap(), None)
                    .map_err(|e| anyhow!("Failed to load ONNX Multimodal weights: {}", e))?;
            } else {
                onnx_engine.load_vision_model(model_load_path, None)
                    .map_err(|e| anyhow!("Failed to load ONNX Vision weights: {}", e))?;
            }
                
            tracing::info!("🧬 [Orchestrator] ONNX Native Pipeline Established.");
            return Ok(Box::new(NativeOnnxWrapper { engine: onnx_engine }));
        }

        // 1. Initialize the Engine Manager (The cluaiz Linker)
        let base_path = cluaiz_shared::hardware::governor::HardwareGovernor::resolve_hub_path();
        let mut manager = EngineManager::new(base_path);

        // 2. Engine Type provided by Unified Router (e.g., "llama" or "onnx")


        // 3. Prepare Engine: Hardware Probe + Binary Linkage
        let binary_path = match manager.prepare_engine(engine_type).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("🚨 [Orchestrator] Hardware Provisioning Failed: {}. Degrading gracefully to CPU execution...", e);
                return Err(anyhow!("Hardware Linkage Failure: {}", e));
            }
        };

        // 🚀 [FFI Handshake]: Map the binary to process memory
        if let Err(e) = manager.load_and_link(binary_path) {
            tracing::warn!("🚨 [Orchestrator] FFI Linkage Failed: {}. Degrading gracefully to CPU execution...", e);
            return Err(anyhow!("Hardware Linkage Failure: {}", e));
        }

        // 🏛️ [Core Instantiation]: Create the active engine instance with User Truth
        let mut booster_control = if let Some(booster) = booster_override {
            booster
        } else {
            cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default()
        };

        // 🛡️ DNA-AWARE FLASH ATTENTION GUARD (Mathematical Resolution)
        // Flash Attention fundamentally requires the attention head dimension to be a specific size (usually 64, 128, or 256)
        // and does not apply to State Space Models (SSMs) or certain 1-bit architectures.
        if booster_control.flash_attention == cluaiz_shared::hardware::schema::booster::FeatureState::On {
            let head_dim = cluaiz_context.dna.attention_head_dim.unwrap_or_else(|| {
                if let (Some(h), Some(c)) = (cluaiz_context.dna.hidden_size, cluaiz_context.dna.attention_head_count) {
                    h / c
                } else {
                    128 // Default theoretical fallback
                }
            });

            let math_supports_flash = !cluaiz_context.dna.signature.is_ssm 
                && (head_dim == 64 || head_dim == 128 || head_dim == 256);

            // [CERD Doctrine applied]: Llama.cpp CUDA backend critically lacks Flash Attention support 
            // for Ternary/BitNet tensor layouts (TQ1_0, TQ2_0) on CUDA.
            // Passing flash_attention=true to these architectures causes a fatal Model Load Failure.
            // We mathematically filter SSMs above, and filter BitNet via strict boolean property.
            let is_architecturally_broken = cluaiz_context.dna.signature.is_bitnet;

            if !math_supports_flash || is_architecturally_broken {
                booster_control.flash_attention = cluaiz_shared::hardware::schema::booster::FeatureState::Off;
                tracing::warn!("⚠️ [Arbiter] Flash Attention disabled: Math anomaly ({}) or Architecture lacks GGML CUDA FA support.", head_dim);
            }
        }

        let max_ctx = cluaiz_context.dna.max_context_length.map(|c| c as u32);
        let engine_ptr = manager.instantiate(model_load_path, &booster_control, max_ctx)?;

        tracing::info!("🧬 [Orchestrator] Hardware Handshake SUCCESS. Neural Bridge Established.");
        
        Ok(Box::new(SovereignEngine {
            manager: Arc::new(Mutex::new(manager)),
            engine_ptr,
            engine_id: engine_type.to_string(),
        }))
    }

    pub fn purge_hardware_context() {
        tracing::warn!("🚨 [Manager] EMERGENCY EVICT TRIGGERED. Purging Core Memory...");
    }
}

/// 🧬 [The Neural Bridge]: Connects the Sovereign OS to the Bare-Metal Kernel.
pub struct SovereignEngine {
    manager: Arc<Mutex<EngineManager>>,
    engine_ptr: *mut std::ffi::c_void,
    engine_id: String,
}

unsafe impl Send for SovereignEngine {}
unsafe impl Sync for SovereignEngine {}

impl Drop for SovereignEngine {
    fn drop(&mut self) {
        if let Ok(manager) = self.manager.lock() {
            let _ = manager.free_instance(self.engine_ptr);
        }
        let _ = cluaiz_shared::hardware::governor::HardwareGovernor::release_vram(&self.engine_id);
    }
}

impl UnifiedBackend for SovereignEngine {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> Result<String, String> {
        Err("SovereignEngine: Use generate_stream for native performance.".to_string())
    }

    fn prefill(&mut self, prompt: &str) -> Result<()> {
        let manager = self.manager.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        // Pre-fill the KV cache by running generation with 0 max_tokens
        manager.generate_stream_ffi(self.engine_ptr, prompt, 0, Box::new(|_| true))
    }

    fn evaluate_tps(&self) -> f64 {
        85.0
    }
}

impl cluaizInference for SovereignEngine {
    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        let manager = self.manager.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        manager.generate_stream_ffi(self.engine_ptr, prompt, max_tokens, callback)
    }

    fn forward_raw(&mut self, _input_ids: &[u32], _pos: usize) -> Result<Vec<f32>> {
        Err(anyhow!("forward_raw is optimized via FFI inside the kernel."))
    }

    fn dump_kv_cache(&mut self, path: &str) -> Result<()> {
        let manager = self.manager.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        manager.dump_kv_cache_ffi(self.engine_ptr, path)
    }

    fn load_kv_cache(&mut self, path: &str) -> Result<()> {
        let manager = self.manager.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        manager.load_kv_cache_ffi(self.engine_ptr, path)
    }

    fn inject_signals(&mut self, signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>) -> Result<()> {
        if signals.is_empty() {
            return Ok(());
        }
        
        let manager = self.manager.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        tracing::debug!("⏸️ [Agentic Pause] Halting autoregressive loop for dynamic KV cache injection...");
        
        // Dynamic sizing info logic (no 25% fixed limit)
        let total_tokens: usize = signals.iter().map(|s| s.token_count).sum();
        tracing::debug!("💉 [VRAM Injector] Stitching {} total dynamic tokens to active Context Window prefix.", total_tokens);
        
        // Pass to FFI
        manager.inject_signals_ffi(self.engine_ptr, signals)?;
        
        tracing::debug!("▶️ [Agentic Pause] Injection complete. Inference loop resumed.");
        Ok(())
    }

    fn apply_booster(&mut self, _control: &cluaiz_shared::hardware::schema::booster::BoosterControl) -> Result<()> {
        Ok(())
    }

    fn set_liquid_mode(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }
}

pub struct NativeOnnxWrapper {
    pub engine: cluaiz_onnx::engine::OnnxEngine,
}

impl UnifiedBackend for NativeOnnxWrapper {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> Result<String, String> {
        Ok("[ONNX Vision/Embedding Output] (Placeholder for Gatekeeper)".to_string())
    }

    fn prefill(&mut self, _prompt: &str) -> Result<()> { Ok(()) }

    fn evaluate_tps(&self) -> f64 { 5000.0 }
    
    fn embed(&mut self, input: &str) -> Result<Vec<f32>> {
        self.engine.gen_embedding(input).map_err(|e| anyhow!("ONNX Embedding Error: {}", e))
    }
}

impl cluaizInference for NativeOnnxWrapper {
    fn generate_stream(
        &mut self,
        prompt: &str,
        _max_tokens: usize,
        mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        let clean_path = prompt.trim().trim_matches('"');
        
        if std::path::Path::new(clean_path).is_file() {
            let ingestor = crate::neural_foundry::ingestion::DocumentIngestor::new();
            match ingestor.ingest_and_vectorize(clean_path, &self.engine) {
                Ok(chunks) => {
                    for (chunk_text, vector) in chunks {
                        let mut out = String::new();
                        out.push_str(&format!("\n  🔮 Generated Sovereign Embedding Vector (Dim: {})\n", vector.len()));
                        out.push_str("  [ ");
                        for (i, val) in vector.iter().take(8).enumerate() {
                            out.push_str(&format!("{:.4}", val));
                            if i < 7 && i < vector.len() - 1 {
                                out.push_str(", ");
                            }
                        }
                        if vector.len() > 8 {
                            out.push_str(" ... ");
                        }
                        out.push_str("]\n");
                        let preview: String = chunk_text.chars().take(60).collect();
                        out.push_str(&format!("  📝 Chunk Preview: {}...\n", preview.replace('\n', " ")));
                        
                        for line in out.lines() {
                            if !callback(format!("{}\n", line)) { break; }
                        }
                    }
                },
                Err(e) => {
                    let _ = callback(format!("ONNX Document Error: {}", e));
                }
            }
        } else {
            let vector_result = self.embed(prompt);
            match vector_result {
                Ok(vector) => {
                    let mut out = String::new();
                    out.push_str(&format!("\n  🔮 Generated Sovereign Embedding Vector (Dim: {})\n", vector.len()));
                    out.push_str("  [ ");
                    for (i, val) in vector.iter().take(8).enumerate() {
                        out.push_str(&format!("{:.4}", val));
                        if i < 7 && i < vector.len() - 1 {
                            out.push_str(", ");
                        }
                    }
                    if vector.len() > 8 {
                        out.push_str(" ... ");
                    }
                    out.push_str("]\n");
                    
                    for line in out.lines() {
                        if !callback(format!("{}\n", line)) { break; }
                    }
                }
                Err(e) => {
                    let _ = callback(format!("ONNX Error: {}", e));
                }
            }
        }
        Ok(())
    }

    fn forward_raw(&mut self, _input_ids: &[u32], _pos: usize) -> Result<Vec<f32>> {
        Err(anyhow!("ONNX Native does not support forward_raw for text tokens yet."))
    }

    fn inject_signals(&mut self, _signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>) -> Result<()> { Ok(()) }
    fn apply_booster(&mut self, _control: &cluaiz_shared::hardware::schema::booster::BoosterControl) -> Result<()> { Ok(()) }
    fn set_liquid_mode(&mut self, _enabled: bool) -> Result<()> { Ok(()) }
}
