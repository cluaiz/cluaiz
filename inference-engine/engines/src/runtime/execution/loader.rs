use std::path::Path;
use anyhow::{Result, anyhow};
use crate::runtime::execution::hub::HardwareOrchestrator as CoreHub;
use cluaiz_shared::{ModelWeightsWrapper, cluaizContext, StructuralDNA, TemplateManager};
use cluaiz_shared::utils::GGUFProber;
use cluaiz_shared::hardware::schema::booster::FeatureState;

/// GGUFLoader: Lightweight orchestrator for quantized Core models.
pub struct GGUFLoader;

impl GGUFLoader {
    pub async fn load_model(path: &Path, _hf_repo: &str) -> Result<(ModelWeightsWrapper, Option<u32>)> {
        // 1. Detect Architecture via Native Prober (Zero Framework Bloat)
        let (metadata, tensor_infos, _tensor_count) = GGUFProber::probe(path)
            .map_err(|e| anyhow!("Native Probe Failure: {}", e))?;
        
        let arch = metadata.get("general.architecture")
            .ok_or_else(|| anyhow!("Registry Alert: Architecture metadata missing in GGUF file."))?;
        
        // [cluaiz CLEAN]: Replaced tracing::info with println for editor stability
        cluaiz_shared::dev_info!("🔍 Autonomous Discovery: Probed architecture '{}' via Native Prober", arch);

        // 2. Extract Special Tokens (Resilient Handshake)
        let bos_token_id = metadata.get("tokenizer.ggml.bos_token_id")
            .and_then(|v| v.parse::<u32>().ok());

        // 3. Identify Metadata Assets (Structural DNA)
        let model_dir = path.parent().ok_or_else(|| anyhow!("Invalid model path structure."))?;
        let dna_path = model_dir.join("structural_dna.json");
        let mut architectural_dna = if dna_path.exists() {
             StructuralDNA::load(&dna_path)
                .map_err(|load_err| anyhow!("cluaiz Boot Failure: DNA corrupt. Detail: {}", load_err))?
        } else {
             StructuralDNA::default()
        };

        // 🧠 Stage 1/2/3: Arbiter Routing Logic (Speculative Decoding)
        let booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
        if booster.speculative_decoding != FeatureState::Off {
            let has_native_mtp = GGUFProber::check_native_mtp(&tensor_infos);
            if has_native_mtp {
                cluaiz_shared::dev_info!("🔥 [Arbiter] Native MTP detected in binary headers. Engaging High-Fidelity MTP Loop.");
                architectural_dna.dynamic_attributes.insert("speculative_mode".to_string(), "native_mtp".to_string());
            } else {
                cluaiz_shared::dev_info!("🛡️ [Arbiter] No Native MTP. Fallback Path Triggered.");
                // Stage 3: Eagle vs Lookahead based on VRAM (Simulation using available metadata)
                // In an actual scenario, VRAM is measured in DNA discovery, but here we assume threshold
                if architectural_dna.vram_headroom_gb <= 0.1 && architectural_dna.vram_headroom_gb > 0.0 {
                    cluaiz_shared::dev_info!("⚡ [Arbiter] VRAM Choked. Engaging Draftless Lookahead Decoding.");
                    architectural_dna.dynamic_attributes.insert("speculative_mode".to_string(), "lookahead".to_string());
                } else {
                    cluaiz_shared::dev_info!("🦅 [Arbiter] VRAM Space Available. Engaging cluaiz Eagle Decoding (2.5x Boost).");
                    architectural_dna.dynamic_attributes.insert("speculative_mode".to_string(), "eagle".to_string());
                }
            }
        } else {
             architectural_dna.dynamic_attributes.insert("speculative_mode".to_string(), "off".to_string());
        }

        // Tokenizer setup removed since GGUF natively extracts it.

        // 🧬 cluaiz ACTIVATION: Dynamic Context Bootstrapping
        let cluaiz_context = cluaizContext::boot(
            architectural_dna,
            TemplateManager::default()
        );
 
        // 4. Delegate Instantiation to the Core Hub (Universal DNA Dispatch)
        let model = CoreHub::instantiate(path.to_string_lossy().as_ref(), "gguf", cluaiz_context).await?;
  
        Ok((model, bos_token_id))
    }
}
