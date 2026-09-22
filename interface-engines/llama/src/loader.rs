//! Sovereign Implementation B: Core Loader.

use engine_core::backend::signature::{BackendType, KernelSignature};
use engine_core::backend::context::EngineContext;
use engine_core::backend::traits::ModelWeightsWrapper;
use std::sync::Arc;
use crate::config::OptimizationConfig;

pub struct RuntimeBLoader;

impl RuntimeBLoader {
    pub fn register_drivers(mut register_fn: impl FnMut(BackendType, KernelSignature, engine_core::ArcConstructor)) -> Result<(), String> {
        let patterns = vec!["uniform", "asymmetric"];

        for pattern in patterns {
            let signature = KernelSignature {
                has_experts: false,
                is_asymmetric: pattern == "asymmetric",
                is_multimodal: true,
                is_heterogeneous: true,
                is_bitnet: false,
                is_ssm: false,
                head_pattern: pattern.into(),
                activation: "silu".into(),
            };

            register_fn(
                BackendType::RuntimeB,
                signature,
                Arc::new(
                    |model_load_path: &str,
                     sovereign_context: EngineContext| {
                        // Dynamic param resolution (Handled autonomously by BoosterConfig)
                        
                        let engine = crate::RuntimeB::new(model_load_path, sovereign_context);
                        Ok(Box::new(engine) as ModelWeightsWrapper)
                    },
                ) as engine_core::ArcConstructor,
            );

        }
        Ok(())
    }
}
