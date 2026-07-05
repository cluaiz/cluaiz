//! 🔮 Embedding Generator: Lazily loaded ONNX text embedding generator with safe fallbacks.

use crate::neural_foundry::security::permission_schema::PermissionSchema;
use cluaiz_onnx::engine::OnnxEngine;
use neural_core::interfaces::router_contract::EmbeddingDriver;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::path::PathBuf;

static GLOBAL_EMBEDDING_ENGINE: Lazy<Mutex<Option<OnnxEngine>>> = Lazy::new(|| Mutex::new(None));

pub struct EmbeddingGenerator;

static INIT_ATTEMPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl EmbeddingGenerator {
    fn init_engine() -> Option<OnnxEngine> {
        let schema = PermissionSchema::load();
        let model_id = schema.get_active_embedding_model()?;
        
        let formatted_model_id = model_id.replace(":", "-");
        let model_dir = cluaiz_shared::environment::EnvironmentManager::current()
            .ensure_embedding_models_dir()
            .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().embedding_models_dir())
            .join(&formatted_model_id);
        
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            tracing::warn!("Embedding model files missing at {:?}. Fallback active.", model_dir);
            return None;
        }

        tracing::info!("Loading ONNX Embedding Model: {}...", model_id);
        let mut engine = OnnxEngine::new().ok()?;
        engine.load_text_model(&model_path.to_string_lossy(), &tokenizer_path.to_string_lossy(), None).ok()?;

        Some(engine)
    }

    /// Generates a float vector from text.
    /// If model fails to load or infer, it returns a safe zero-filled fallback vector.
    pub fn generate_vector(text: &str) -> Vec<f32> {
        let mut lock = match GLOBAL_EMBEDDING_ENGINE.lock() {
            Ok(l) => l,
            Err(_) => return vec![0.0f32; 16],
        };

        if lock.is_none() && !INIT_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed) {
            INIT_ATTEMPTED.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(engine) = Self::init_engine() {
                *lock = Some(engine);
            }
        }

        if let Some(engine) = &*lock {
            match engine.gen_embedding(text) {
                Ok(full_vec) => full_vec,
                Err(e) => {
                    tracing::warn!("ONNX embedding inference failed: {:?}. Using fallback zero-vector.", e);
                    vec![0.0f32; 16]
                }
            }
        } else {
            vec![0.0f32; 16]
        }
    }

    /// Generates a full float vector from text for semantic routing.
    pub fn generate_full_vector(text: &str) -> Option<Vec<f32>> {
        let mut lock = match GLOBAL_EMBEDDING_ENGINE.lock() {
            Ok(l) => l,
            Err(_) => return None,
        };

        if lock.is_none() && !INIT_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed) {
            INIT_ATTEMPTED.store(true, std::sync::atomic::Ordering::Relaxed);
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
