use anyhow::Result;
use ort::session::Session;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokenizers::Tokenizer;

/// ONNX Multimodal Router (Core Engine)
pub struct OnnxEngine {
    // 🏊 Session Pool: N concurrent sessions for parallel embedding requests
    pub(crate) session_pool: Vec<Arc<std::sync::Mutex<Session>>>,
    pub(crate) tokenizer: Option<Arc<Tokenizer>>,
    // 🔢 Active Inference Counter: tracks in-flight requests for safe hot swap
    pub(crate) active_inferences: Arc<AtomicUsize>,
    // 🧠 KV Cache for Chat Generation
    pub(crate) active_kv_cache: Option<Vec<(Vec<usize>, Vec<f32>)>>,
}

impl OnnxEngine {
    pub fn new() -> Result<Self> {
        // Initialize ONNX Runtime environment implicitly.
        ort::init().with_name("cluaiz_onnx_env").commit();

        tracing::info!("🧿 [ONNX] Runtime initialized. Ready to load models via API.");

        Ok(Self {
            session_pool: Vec::new(),
            tokenizer: None,
            active_inferences: Arc::new(AtomicUsize::new(0)),
            active_kv_cache: None,
        })
    }

    /// Acquire a session from the pool.
    /// Tries to find a free (non-blocked) session first; falls back to the first session.
    pub(crate) fn acquire_session(
        &self,
    ) -> Result<Arc<std::sync::Mutex<Session>>, neural_core::interfaces::router_contract::EngineError>
    {
        if self.session_pool.is_empty() {
            return Err(
                neural_core::interfaces::router_contract::EngineError::Internal(
                    "No ONNX sessions in pool — model not loaded.".into(),
                ),
            );
        }
        // Try to find an immediately-free session (non-blocking check)
        for session_arc in &self.session_pool {
            if session_arc.try_lock().is_ok() {
                return Ok(session_arc.clone());
            }
        }
        // All busy — return first one (caller will block until it's free)
        tracing::warn!(
            "⚠️ [ONNX Pool] All {} sessions busy, caller will block.",
            self.session_pool.len()
        );
        Ok(self.session_pool[0].clone())
    }

    /// Dynamically load a model from disk into the ONNX Runtime (e.g. bge-m3-quantized.onnx).
    /// Builds a pool of N sessions for concurrent embedding requests.
    pub fn load_text_model(
        &mut self,
        model_path: &str,
        tokenizer_path: &str,
        booster: Option<cluaiz_shared::hardware::schema::booster::cluaizBoosterContext>,
    ) -> Result<()> {
        // 🔒 SINGLETON OWNERSHIP GUARD (CERD Rule: exactly one owner)
        if !self.session_pool.is_empty() {
            let active = self.active_inferences.load(Ordering::Relaxed);
            if active > 0 {
                tracing::warn!("⚠️ [ONNX] {} active inference(s) in flight during eviction. Sessions are Arc-protected and will complete safely.", active);
            }
            tracing::warn!(
                "⚠️ [ONNX] Evicting {} session(s) before loading: {}",
                self.session_pool.len(),
                model_path
            );
            self.session_pool.clear();
            self.tokenizer = None;
        }
        tracing::info!("📦 [ONNX] Loading model from: {}", model_path);

        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING
        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = false;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            // DYNAMIC MODEL SIZE CHECK (No hardcoded fallback numbers)
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.2 // 20% buffer for KV cache & context
            } else {
                tracing::warn!("📡 [Telemetry] Could not read ONNX model size. Auto-falling back to CPU for safety.");
                f64::MAX // Force CPU fallback by making required VRAM infinite
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB, Req: {:.1}GB). Routing ONNX to GPU.", free_vram, required_vram);
                use_gpu = true;
            } else if required_vram != f64::MAX {
                tracing::warn!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB, Req: {:.1}GB). Auto-falling back ONNX to CPU AVX.", free_vram, required_vram);
            }
        }

        // Booster Override (GGUF Compatibility)
        if let Some(b) = &booster {
            if b.n_gpu_layers == 0 {
                use_gpu = false;
                tracing::info!("⚙️ [Booster] Force CPU mode requested by user.");
            } else if b.n_gpu_layers > 0 {
                use_gpu = true;
                tracing::info!(
                    "⚙️ [Booster] Force GPU mode requested by user (Layers: {}).",
                    b.n_gpu_layers
                );
            }
        }

        // ONNX Metadata Override
        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.execution_provider.eq_ignore_ascii_case("CPU") {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user.");
        } else if onnx_meta.execution_provider.eq_ignore_ascii_case("Auto") {
            tracing::info!("⚙️ [ONNX Config] Auto mode active (Relying on telemetry).");
        }

        let total_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // Pool size: min(cores, 4). Each session gets equal thread share.
        let pool_size = total_threads.min(4).max(1);
        let intra_threads_per_session = if onnx_meta.intra_op_num_threads > 0 {
            onnx_meta.intra_op_num_threads
        } else {
            (total_threads / pool_size).max(1)
        };

        let opt_level = match onnx_meta.graph_optimization_level.to_uppercase().as_str() {
            "ORT_DISABLE_ALL" => ort::session::builder::GraphOptimizationLevel::Disable,
            "ORT_ENABLE_BASIC" => ort::session::builder::GraphOptimizationLevel::Level1,
            "ORT_ENABLE_EXTENDED" => ort::session::builder::GraphOptimizationLevel::Level2,
            "ORT_ENABLE_ALL" => ort::session::builder::GraphOptimizationLevel::Level3,
            _ => ort::session::builder::GraphOptimizationLevel::Level3,
        };

        tracing::info!(
            "🏊 [ONNX Pool] Building {} sessions ({} threads each, Opt: {:?}, Profile: {})...",
            pool_size,
            intra_threads_per_session,
            opt_level,
            onnx_meta.enable_profiling
        );

        for i in 0..pool_size {
            let mut builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(opt_level)
                .map_err(|e| anyhow::anyhow!("Opt level error: {:?}", e))?
                .with_intra_threads(intra_threads_per_session)
                .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?;

            if onnx_meta.inter_op_num_threads > 0 {
                builder = builder.with_inter_threads(onnx_meta.inter_op_num_threads)
                    .map_err(|e| anyhow::anyhow!("Inter threads error: {:?}", e))?;
            }

            builder = builder.with_memory_pattern(onnx_meta.enable_mem_pattern)
                .map_err(|e| anyhow::anyhow!("Memory pattern error: {:?}", e))?;

            let is_parallel = onnx_meta.execution_mode.eq_ignore_ascii_case("ORT_PARALLEL");
            builder = builder.with_parallel_execution(is_parallel)
                .map_err(|e| anyhow::anyhow!("Execution mode error: {:?}", e))?;

            builder = builder.with_deterministic_compute(onnx_meta.use_deterministic_compute)
                .map_err(|e| anyhow::anyhow!("Deterministic compute error: {:?}", e))?;

            if onnx_meta.enable_profiling {
                builder = builder.with_profiling(r"onnx_profile")
                    .map_err(|e| anyhow::anyhow!("Profiling config error: {:?}", e))?;
            }

            let cpu_ep = ort::execution_providers::CPUExecutionProvider::default()
                .with_arena_allocator(onnx_meta.enable_cpu_mem_arena)
                .build();

            if use_gpu {
                let mut cuda_ep = ort::execution_providers::CUDAExecutionProvider::default();
                if onnx_meta.gpu_mem_limit_bytes > 0 {
                    cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
                }
                
                let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                    "kSameAsRequested" => ort::execution_providers::ArenaExtendStrategy::SameAsRequested,
                    _ => ort::execution_providers::ArenaExtendStrategy::NextPowerOfTwo,
                };
                cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

                builder = builder
                    .with_execution_providers([
                        cuda_ep.build(),
                        ort::execution_providers::CoreMLExecutionProvider::default().build(),
                        cpu_ep,
                    ])
                    .map_err(|e| anyhow::anyhow!("Execution Provider error: {:?}", e))?;
            } else {
                builder = builder
                    .with_execution_providers([cpu_ep])
                    .map_err(|e| anyhow::anyhow!("CPU Provider error: {:?}", e))?;
            }

            let session = builder
                .commit_from_file(model_path)
                .map_err(|e| anyhow::anyhow!("ORT Session [{}] failed: {}", i, e))?;

            self.session_pool
                .push(Arc::new(std::sync::Mutex::new(session)));
        }

        if use_gpu {
            tracing::info!("🚀 [ONNX] CUDA Execution Provider ready for pool sessions.");
        }

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Tokenizer failed: {}", e))?;
        self.tokenizer = Some(Arc::new(tokenizer));

        tracing::info!(
            "✅ [ONNX Pool] {} text sessions loaded and ready.",
            pool_size
        );
        Ok(())
    }

    /// Dynamically load a vision embedding model (like CLIP) into ONNX Runtime.
    /// Vision models are large — pool size is fixed at 1 to conserve VRAM.
    pub fn load_vision_model(
        &mut self,
        model_path: &str,
        booster: Option<cluaiz_shared::hardware::schema::booster::cluaizBoosterContext>,
    ) -> Result<()> {
        // 🔒 SINGLETON OWNERSHIP GUARD (CERD Rule: exactly one owner)
        if !self.session_pool.is_empty() {
            let active = self.active_inferences.load(Ordering::Relaxed);
            if active > 0 {
                tracing::warn!(
                    "⚠️ [ONNX] {} active vision inference(s) in flight during eviction.",
                    active
                );
            }
            tracing::warn!(
                "⚠️ [ONNX] Evicting vision session pool before loading: {}",
                model_path
            );
            self.session_pool.clear();
        }
        tracing::info!("👁️ [ONNX] Loading Vision Model from: {}", model_path);

        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING (Same as text)
        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = false;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            // DYNAMIC MODEL SIZE CHECK (No hardcoded fallback numbers)
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.2 // 20% buffer for KV cache & context
            } else {
                tracing::warn!("📡 [Telemetry] Could not read Vision model size. Auto-falling back to CPU for safety.");
                f64::MAX // Force CPU fallback
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB, Req: {:.1}GB). Routing Vision Model to GPU.", free_vram, required_vram);
                use_gpu = true;
            } else if required_vram != f64::MAX {
                tracing::warn!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB, Req: {:.1}GB). Auto-falling back Vision Model to CPU AVX.", free_vram, required_vram);
            }
        }

        // Booster Override (GGUF Compatibility)
        if let Some(b) = &booster {
            if b.n_gpu_layers == 0 {
                use_gpu = false;
                tracing::info!("⚙️ [Booster] Force CPU Vision mode requested by user.");
            } else if b.n_gpu_layers > 0 {
                use_gpu = true;
                tracing::info!(
                    "⚙️ [Booster] Force GPU Vision mode requested by user (Layers: {}).",
                    b.n_gpu_layers
                );
            }
        }

        // ONNX Metadata Override
        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.execution_provider.eq_ignore_ascii_case("CPU") {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user.");
        } else if onnx_meta.execution_provider.eq_ignore_ascii_case("Auto") {
            tracing::info!("⚙️ [ONNX Config] Auto mode active (Relying on telemetry).");
        }

        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let mut builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Vision Session builder error: {:?}", e))?
            .with_intra_threads(threads)
            .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?;

        if use_gpu {
            // Attempt to attach CUDA or CoreML. If neither is available, it gracefully falls back to CPU.
            builder = builder
                .with_execution_providers([
                    ort::execution_providers::CUDAExecutionProvider::default().build(),
                    ort::execution_providers::CoreMLExecutionProvider::default().build(),
                ])
                .map_err(|e| anyhow::anyhow!("Execution Provider error: {:?}", e))?;
        }

        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("ORT Vision Session failed: {}", e))?;

        if use_gpu {
            tracing::info!("🚀 [ONNX] CUDA Execution Provider ready for vision session.");
        }

        self.session_pool
            .push(Arc::new(std::sync::Mutex::new(session)));
        tracing::info!("✅ [ONNX] Vision session loaded (pool size: 1).");
        Ok(())
    }
}

use neural_core::interfaces::router_contract::{EmbeddingDriver, EngineError, Modality};

impl EmbeddingDriver for OnnxEngine {
    fn gen_embedding(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        self.execute_text_embedding(text)
    }

    fn gen_multimodal_embedding(
        &self,
        bytes: &[u8],
        modality: Modality,
        instruction: Option<String>
    ) -> Result<Vec<f32>, EngineError> {
        if let Some(ins) = instruction {
            tracing::info!("OnnxEngine: Received multimodal instruction: {}", ins);
        }
        match modality {
            Modality::Image => self.execute_vision_embedding(bytes),
            _ => Err(EngineError::UnsupportedModality(
                "Only Modality::Image is currently supported in Vision ONNX Engine".to_string(),
            )),
        }
    }
}
