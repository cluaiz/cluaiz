//! 🔮 Embedding Generator: Lazily loaded ONNX text embedding generator with safe fallbacks.

use crate::neural_foundry::security::permission_schema::PermissionSchema;
use cluaize_onnx::engine::OnnxEngine;
use neural_core::interfaces::router_contract::EmbeddingDriver;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::path::PathBuf;

static GLOBAL_EMBEDDING_ENGINE: Lazy<Mutex<Option<OnnxEngine>>> = Lazy::new(|| Mutex::new(None));

pub struct EmbeddingGenerator;

impl EmbeddingGenerator {
    fn init_engine() -> Option<OnnxEngine> {
        let schema = PermissionSchema::load();
        let model_id = schema.get_active_embedding_model()?;
        
        let formatted_model_id = model_id.replace(":", "-");
        let model_dir = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_embedding_models_dir()
            .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().embedding_models_dir())
            .join(&formatted_model_id);
        
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            tracing::warn!("Embedding model files missing at {:?}. Fallback active.", model_dir);
            return None;
        }

        tracing::info!("Loading ONNX Embedding Model: {}...", model_id);
        let mut engine = OnnxEngine::new().ok()?;
        engine.load_text_model(&model_path.to_string_lossy(), &tokenizer_path.to_string_lossy()).ok()?;

        Some(engine)
    }

    /// Generates a 16-dimensional float vector from text.
    /// If model fails to load or infer, it returns a safe zero-filled fallback vector [0.0; 16].
    pub fn generate_vector(text: &str) -> [f32; 16] {
        let mut vector = [0.0f32; 16];
        let mut lock = match GLOBAL_EMBEDDING_ENGINE.lock() {
            Ok(l) => l,
            Err(_) => return vector,
        };

        if lock.is_none() {
            if let Some(engine) = Self::init_engine() {
                *lock = Some(engine);
            }
        }

        if let Some(engine) = &*lock {
            match engine.gen_embedding(text) {
                Ok(full_vec) => {
                    for (i, &val) in full_vec.iter().take(16).enumerate() {
                        vector[i] = val;
                    }
                }
                Err(e) => {
                    tracing::warn!("ONNX embedding inference failed: {:?}. Using fallback zero-vector.", e);
                }
            }
        } else {
            tracing::debug!("Embedding engine not initialized. Using fallback zero-vector.");
        }
        vector
    }

    /// Generates a full float vector from text for semantic routing.
    pub fn generate_full_vector(text: &str) -> Option<Vec<f32>> {
        let mut lock = match GLOBAL_EMBEDDING_ENGINE.lock() {
            Ok(l) => l,
            Err(_) => return None,
        };

        if lock.is_none() {
            if let Some(engine) = Self::init_engine() {
                *lock = Some(engine);
            }
        }

        if let Some(engine) = &*lock {
            engine.gen_embedding(text).ok()
        } else {
            None
        }
    }
}
