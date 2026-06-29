use std::pin::Pin;
use cluaiz_shared::backend::signature::KernelSignature;

/// The universal neural stream format.
pub type TokenStream = Pin<Box<dyn std::future::Future<Output = String> + Send>>;

/// 📖 SovereignEngine
/// The universal contract that every engine (Llama/Native) must obey.
pub trait SovereignEngine: Send + Sync {
    /// Boot up the engine, claim VRAM/RAM, and map SSD paths.
    fn init_engine(&mut self, signature: &KernelSignature) -> anyhow::Result<()>;
    
    /// The main inference pipeline. Must support zero-IPC direct memory writing.
    fn generate_tokens(&self, prompt: &str) -> anyhow::Result<TokenStream>;
    
    /// Safely dump weights and return memory to the OS.
    fn unload(&mut self) -> anyhow::Result<()>;

    /// 🚀 Booster Sync: Applies hardware-level optimization flags (TurboQuant, KV-Cache, etc.)
    fn apply_booster(&mut self, _control: &cluaiz_shared::hardware::schema::booster::BoosterControl) -> anyhow::Result<()> {
        Ok(())
    }

    /// 🌊 Liquid Execution: Activates adaptive context density.
    fn set_liquid_mode(&mut self, _enabled: bool) -> anyhow::Result<()> {
        Ok(())
    }

    /// 🧠 JEPA Predictor: Returns latent state predictions for future tokens.
    fn predict_latent(&mut self, _input_ids: &[u32]) -> anyhow::Result<Vec<f32>> {
        Err(anyhow::anyhow!("JEPA not supported by this engine backend."))
    }
}
