use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use crate::backend::context::cluaizContext;
use crate::backend::traits::ModelWeightsWrapper;

/// ArcConstructor: The factory closure signature for instantiating model architectures.

/// Removed candle-core dependencies to allow for truly agnostic engine backends.
pub type ArcConstructor = std::sync::Arc<dyn Fn(
    &str,                // load_path
    cluaizContext,     // system context
) -> anyhow::Result<ModelWeightsWrapper> + Send + Sync>;


// ─── Kernel Signature (The Mathematical Identity) ──────────────────────────
#[derive(Debug, Clone, PartialEq, Eq, Default, std::hash::Hash, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct KernelSignature {
    pub has_experts: bool,
    pub is_asymmetric: bool,
    pub is_multimodal: bool,
    pub is_heterogeneous: bool,
    pub is_bitnet: bool,
    pub is_ssm: bool, // Mamba/Linear Recurrence
    pub head_pattern: String, 
    pub activation: String,
}

// ─── Core Backend Enums ────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[serde(rename_all = "camelCase")]
pub enum BackendType {

    #[serde(alias = "candlenative")]
    RuntimeA,
    #[serde(alias = "llamacppffi")]
    RuntimeB,
    #[serde(alias = "bitnetnative")]
    RuntimeC,
    #[serde(alias = "tritonkernel")]
    RuntimeD,
    #[serde(alias = "moerouter")]
    SwitchNode,
}

// ─── Global Registration System ──────────────────────────────────────────
pub struct GlobalFeatureRegistry;

impl GlobalFeatureRegistry {
    /// Dispatches a model loader based on the model signature and hardware truth.
    pub fn select_runtime(signature: &KernelSignature, hardware: &crate::hardware::schema::profiles::SiliconTruth) -> BackendType {
        // 🚀 Sovereign Dispatch Logic (Hardware-Aware)
        
        // 1. NPU/TPU Preference: If an AI accelerator is detected, prioritize it.
        if !hardware.accelerators.npus.is_empty() || !hardware.accelerators.tpus.is_empty() {
             tracing::info!("🧠 [Dispatcher] Silicon Awakening: NPU/TPU Detected. Routing to Runtime D (Hardware Native).");
             return BackendType::RuntimeD;
        }

        // 2. VRAM Guard: Check if we have enough VRAM for Runtime B (Llama.cpp/GPU)
        let has_gpu = !hardware.accelerators.gpus.is_empty();
        if has_gpu && signature.is_bitnet {
             tracing::info!("🔩 [Signature] 1-bit architecture on GPU detected. Routing to Runtime B.");
             return BackendType::RuntimeB;
        }

        // 3. BitNet Native (CPU/NPU)
        if signature.is_bitnet {
            return BackendType::RuntimeC;
        }

        // 4. Default Fallbacks
        if signature.is_heterogeneous {
            BackendType::RuntimeA 
        } else if signature.is_asymmetric && has_gpu {
             BackendType::RuntimeB 
        } else {
             BackendType::RuntimeA
        }
    }
}
