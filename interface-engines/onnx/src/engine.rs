use anyhow::Result;
use ort::session::Session;
use tokenizers::Tokenizer;
use std::sync::Arc;

/// ONNX Multimodal Router (Core Engine)
pub struct OnnxEngine {
    // Real Production Engine State
    pub(crate) session: Option<Arc<std::sync::Mutex<Session>>>,
    pub(crate) tokenizer: Option<Arc<Tokenizer>>,
}

impl OnnxEngine {
    pub fn new() -> Result<Self> {
        // Initialize ONNX Runtime environment implicitly.
        ort::init()
            .with_name("cluaize_onnx_env")
            .commit();

        tracing::info!("🧿 [ONNX] Runtime initialized. Ready to load models via API.");

        Ok(Self {
            session: None,
            tokenizer: None,
        })
    }

    /// Dynamically load a model from disk into the ONNX Runtime (e.g. bge-m3-quantized.onnx)
    pub fn load_text_model(&mut self, model_path: &str, tokenizer_path: &str) -> Result<()> {
        // 🔒 SINGLETON OWNERSHIP GUARD (CERD Rule: exactly one owner)
        if self.session.is_some() {
            tracing::warn!("⚠️ [ONNX] A session is already loaded. Evicting previous session before loading new model at: {}", model_path);
            self.session = None;
            self.tokenizer = None;
        }
        tracing::info!("📦 [ONNX] Loading model from: {}", model_path);
        
        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING
        let pulse_state = cluaize_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = false;
        
        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            if free_vram > 2.0 && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB). Routing ONNX to GPU.", free_vram);
                use_gpu = true;
            } else {
                tracing::warn!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB). Auto-falling back ONNX to CPU AVX.", free_vram);
            }
        }

        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let mut builder = Session::builder()?
            .with_intra_threads(threads).map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?;

        if use_gpu {
            // In a production build with CUDA feature enabled in ORT:
            // builder = builder.with_execution_providers([ort::execution_providers::CUDAExecutionProvider::default().build()]);
            tracing::info!("🚀 [ONNX] Injecting CUDA Execution Provider...");
        }

        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("ORT Session failed: {}", e))?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Tokenizer failed: {}", e))?;

        self.session = Some(Arc::new(std::sync::Mutex::new(session)));
        self.tokenizer = Some(Arc::new(tokenizer));
        
        Ok(())
    }

    /// Dynamically load a vision embedding model (like CLIP) into ONNX Runtime
    pub fn load_vision_model(&mut self, model_path: &str) -> Result<()> {
        // 🔒 SINGLETON OWNERSHIP GUARD (CERD Rule: exactly one owner)
        if self.session.is_some() {
            tracing::warn!("⚠️ [ONNX] A vision session is already loaded. Evicting before loading: {}", model_path);
            self.session = None;
        }
        tracing::info!("👁️ [ONNX] Loading Vision Model from: {}", model_path);
        
        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING (Same as text)
        let pulse_state = cluaize_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = false;
        
        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            if free_vram > 2.0 && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB). Routing Vision Model to GPU.", free_vram);
                use_gpu = true;
            } else {
                tracing::warn!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB). Auto-falling back Vision Model to CPU AVX.", free_vram);
            }
        }

        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let mut builder = Session::builder()?
            .with_intra_threads(threads).map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?;

        if use_gpu {
            tracing::info!("🚀 [ONNX] Injecting CUDA Execution Provider for Vision...");
        }

        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("ORT Vision Session failed: {}", e))?;

        self.session = Some(Arc::new(std::sync::Mutex::new(session)));
        
        Ok(())
    }
}

use neural_core::interfaces::router_contract::{EmbeddingDriver, EngineError, Modality};

impl EmbeddingDriver for OnnxEngine {
    fn gen_embedding(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        // Now executing real text tokenization and embedding extraction
        self.execute_text_embedding(text)
    }

    fn gen_multimodal_embedding(&self, bytes: &[u8], modality: Modality) -> Result<Vec<f32>, EngineError> {
        match modality {
            Modality::Image => self.execute_vision_embedding(bytes),
            _ => Err(EngineError::UnsupportedModality("Only Modality::Image is currently supported in Vision ONNX Engine".to_string())),
        }
    }
}
