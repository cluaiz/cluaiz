use anyhow::Result;
/// UnifiedBackend: The foundational interface for all generation engines in the  system.
pub trait UnifiedBackend {
    /// Sequential generation (Legacy/Compatibility)
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> std::result::Result<String, String>;
    
    /// Prefill: Synchronous bulk processing for prompt saturation
    fn prefill(&mut self, prompt: &str) -> Result<()>;
    
    fn evaluate_tps(&self) -> f64;
    
    /// Generate an embedding vector from text or file path (primarily for Vision/ONNX)
    fn embed(&mut self, _input: &str) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("Embedding generation not supported by this backend."))
    }
}

/// cluaizInference: The advanced streaming iteration interface.
pub trait cluaizInference: Send + Sync + UnifiedBackend {
    /// Returns a generic response from a forward pass (implementation dependent)
    fn forward_raw(&mut self, input_ids: &[u32], pos: usize) -> Result<Vec<f32>>;
    
    /// The high-performance streaming protocol for Sovereign Silicon
    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()>;

    /// 🔗 Signal Injection Hook: Injects multiple pre-encoded neural states directly into hardware cache.
    fn inject_signals(&mut self, _signals: Vec<crate::hardware::memory::kv_cache::stitching::cluaizSignal>) -> Result<()> {
        tracing::warn!("⚠️ [Backend] Multi-Signal injection Not Implemented for this kernel.");
        Ok(())
    }

    /// 🚀 Booster Sync: Applies hardware-level optimization flags (TurboQuant, KV-Cache, etc.)
    fn apply_booster(&mut self, _control: &crate::hardware::schema::booster::BoosterControl) -> Result<()> {
        Ok(())
    }

    /// 🌊 Liquid Execution: Activates adaptive context density.
    fn set_liquid_mode(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    /// 🧠 JEPA Predictor: Returns latent state predictions for future tokens.
    fn predict_latent(&mut self, _input_ids: &[u32]) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("JEPA not supported on this silicon."))
    }

    /// 💾 Native Memory Dump: Extracts the actual KV cache buffer to a binary file.
    fn dump_kv_cache(&mut self, _path: &str) -> Result<()> {
        Err(anyhow::anyhow!("KV Cache dumping not implemented on this engine."))
    }

    /// 💾 Load KV Cache from a binary file.
    fn load_kv_cache(&mut self, _path: &str) -> Result<()> {
        Err(anyhow::anyhow!("KV Cache loading not implemented on this engine."))
    }
}

/// Dynamic trait alias bridging generic hardware kernels
pub type ModelWeightsWrapper = Box<dyn cluaizInference + Send + Sync>;


// ─── Expert Dispatcher (MoE Routing Protocol) ──────────────────────────────
pub trait ExpertDispatcher {
    fn route_token(&self, token_id: u32, experts: usize) -> u32;
    fn get_active_vram_offload(&self) -> usize;
}
