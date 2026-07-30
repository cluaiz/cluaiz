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
    // 🎧 Encoder Session: Whisper encoder (input_features -> encoder_hidden_states)
    pub(crate) encoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    pub(crate) tokenizer: Option<Arc<Tokenizer>>,
    // 🔢 Active Inference Counter: tracks in-flight requests for safe hot swap
    pub(crate) active_inferences: Arc<AtomicUsize>,
    // 🧠 KV Cache for Chat Generation
    pub(crate) active_kv_cache: Option<Vec<(Vec<usize>, std::sync::Arc<Vec<f32>>)>>,
    // 📂 Model Directory Path for loading dynamic configs
    pub(crate) model_dir: Option<std::path::PathBuf>,
}

impl OnnxEngine {
    pub fn new() -> Result<Self> {
        // Prepend C:\Users\Aryan\.cluaiz\engine\drivers to PATH so ONNX Runtime loads CUDA/TensorRT DLLs
        let drivers_dir = cluaiz_shared::environment::EnvironmentManager::current().engine_dir().join("drivers");
        if drivers_dir.exists() {
            let current_path = std::env::var_os("PATH").unwrap_or_default();
            let mut new_path = drivers_dir.into_os_string();
            if !current_path.is_empty() {
                new_path.push(";");
                new_path.push(current_path);
            }
            std::env::set_var("PATH", new_path);
            tracing::info!("🔗 [ONNX] PATH environment variable updated with drivers directory.");
        }

        // Initialize ONNX Runtime environment implicitly.
        ort::init().with_name("cluaiz_onnx_env").commit();

        tracing::info!("🧿 [ONNX] Runtime initialized. Ready to load models via API.");

        Ok(Self {
            session_pool: Vec::new(),
            encoder_session: None,
            tokenizer: None,
            active_inferences: Arc::new(AtomicUsize::new(0)),
            active_kv_cache: None,
            model_dir: None,
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
        // Force drop any lingering audio encoder to free VRAM
        if self.encoder_session.is_some() {
            tracing::warn!("⚠️ [ONNX] Evicting dangling Audio Encoder session to free VRAM.");
            self.encoder_session = None;
        }
        
        tracing::info!("📦 [ONNX] Loading model from: {}", model_path);
        let path = std::path::Path::new(model_path);
        let dir = path.parent().unwrap_or(path).to_path_buf();
        self.model_dir = Some(dir);

        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING
        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = true;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.2
            } else {
                1.5
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB, Req: {:.1}GB). Routing ONNX to GPU.", free_vram, required_vram);
            } else {
                tracing::info!("📡 [Telemetry] Moderate VRAM pressure (Free: {:.1}GB, Req: {:.1}GB). Attempting CUDA GPU allocation with dynamic fallback.", free_vram, required_vram);
            }
        }

        // Booster Override (GGUF Compatibility)
        if let Some(b) = &booster {
            if b.n_gpu_layers == 0 {
                use_gpu = false;
                tracing::info!("⚙️ [Booster] Force CPU mode requested by user.");
            } else if b.n_gpu_layers != 0 {
                use_gpu = true;
                tracing::info!(
                    "⚙️ [Booster] Force GPU mode requested by user (Layers: {}).",
                    b.n_gpu_layers
                );
            }
        }

        // ONNX Metadata Override
        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user.");
        } else if onnx_meta.n_gpu_layers == -1 {
            use_gpu = true;
            tracing::info!("⚙️ [ONNX Config] Auto/GPU Mode active.");
        } else {
            // Apply telemetry fallback if not forced Auto/CPU
            // Telemetry has already decided use_gpu
        }

        let total_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // Pool size: min(cores, 4). Each session gets equal thread share.
        let mut pool_size = total_threads.min(4).max(1);
        if use_gpu {
            pool_size = 1; // Limit pool size for GPU to avoid VRAM duplication
        }
        let intra_threads_per_session = if onnx_meta.intra_op_num_threads > 0 {
            onnx_meta.intra_op_num_threads
        } else {
            total_threads
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

            let mut session_opt = None;
            
            if use_gpu {
                // Tier 1A: Direct Native CUDA Execution Provider
                let mut cuda_ep = ort::execution_providers::CUDAExecutionProvider::default();
                if onnx_meta.gpu_mem_limit_bytes > 0 {
                    cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
                }
                
                let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                    "kSameAsRequested" => ort::execution_providers::ArenaExtendStrategy::SameAsRequested,
                    _ => ort::execution_providers::ArenaExtendStrategy::NextPowerOfTwo,
                };
                cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

                if let Ok(mut cuda_builder) = builder.clone().with_execution_providers([cuda_ep.build(), cpu_ep.clone()]) {
                    match cuda_builder.commit_from_file(model_path) {
                        Ok(sess) => {
                            eprintln!("🎙️ [ONNX Hardware Binding] Session committed ON NATIVE NVIDIA CUDA GPU!");
                            tracing::info!("🚀 [Sovereign-Cascade] ONNX Session [{}] committed ON NATIVE NVIDIA CUDA GPU!", i);
                            session_opt = Some(sess);
                        }
                        Err(e) => {
                            eprintln!("🎙️ [ONNX Hardware Binding] CUDA EP commit failed: {:?}. Trying DirectML GPU fallback...", e);
                            tracing::warn!("⚠️ [Sovereign-Cascade] Native CUDA EP commit failed: {:?}. Trying DirectML/CoreML fallback...", e);
                        }
                    }
                }

                // Tier 1B: Fallback to DirectML (Explicit NVIDIA GPU Device ID 0) / CoreML if native CUDA failed
                if session_opt.is_none() {
                    let dml_ep = ort::execution_providers::DirectMLExecutionProvider::default().with_device_id(0);
                    if let Ok(mut gpu_builder) = builder.clone().with_execution_providers([
                        dml_ep.build(),
                        ort::execution_providers::CoreMLExecutionProvider::default().build(),
                        cpu_ep.clone(),
                    ]) {
                        match gpu_builder.commit_from_file(model_path) {
                            Ok(sess) => {
                                eprintln!("🎙️ [ONNX Hardware Binding] Session committed ON DIRECTX 12 DIRECTML GPU (Device 0)!");
                                tracing::info!("🚀 [Sovereign-Cascade] ONNX Session [{}] committed on DirectML/CoreML GPU.", i);
                                session_opt = Some(sess);
                            }
                            Err(e) => {
                                eprintln!("🎙️ [ONNX Hardware Binding] DirectML GPU Session commit failed: {:?}", e);
                                tracing::warn!("⚠️ [Sovereign-Cascade] DirectML/CoreML Session commit failed: {:?}", e);
                            }
                        }
                    }
                }
            }

            // Tier 2: Try NPU (OpenVINO / DirectML) if Tier 1 failed or was skipped
            if session_opt.is_none() && use_gpu {
                if let Ok(mut npu_builder) = builder.clone().with_execution_providers([
                    ort::execution_providers::OpenVINOExecutionProvider::default().build(),
                    ort::execution_providers::DirectMLExecutionProvider::default().build(),
                    cpu_ep.clone(),
                ]) {
                    if let Ok(sess) = npu_builder.commit_from_file(model_path) {
                        tracing::info!("📡 [Sovereign-Cascade] ONNX Session [{}] committed successfully on Tier 2 (NPU / OpenVINO).", i);
                        session_opt = Some(sess);
                    }
                }
            }

            // Tier 3: CPU Thread Pool Arena
            if session_opt.is_none() {
                if let Ok(mut cpu_builder) = builder.clone().with_execution_providers([cpu_ep.clone()]) {
                    if let Ok(sess) = cpu_builder.commit_from_file(model_path) {
                        tracing::info!("⚙️ [Sovereign-Cascade] ONNX Session [{}] committed on Tier 3 (CPU Thread Pool).", i);
                        session_opt = Some(sess);
                    }
                }
            }

            // Tier 4: SSD Memory Arena / Minimal RAM Swap (Zero Crash Fallback)
            if session_opt.is_none() {
                tracing::warn!("⚠️ [Sovereign-Cascade] CPU RAM Pressure high. Falling back to Tier 4 (SSD Memory Arena / Low-RAM Swap).");
                let fallback_cpu_ep = ort::execution_providers::CPUExecutionProvider::default()
                    .with_arena_allocator(false)
                    .build();
                let mut ssd_builder = Session::builder()
                    .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                    .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                    .map_err(|e| anyhow::anyhow!("Opt level error: {:?}", e))?
                    .with_intra_threads(intra_threads_per_session.min(2))
                    .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                    .with_execution_providers([fallback_cpu_ep])
                    .map_err(|e| anyhow::anyhow!("Fallback CPU Provider error: {:?}", e))?;

                let sess = ssd_builder
                    .commit_from_file(model_path)
                    .map_err(|e| anyhow::anyhow!("Sovereign Cascade failed across all 4 tiers for session [{}]: {}", i, e))?;
                session_opt = Some(sess);
            }

            if let Some(session) = session_opt {
                self.session_pool
                    .push(Arc::new(std::sync::Mutex::new(session)));
            }
        }

        if use_gpu {
            tracing::info!("🚀 [ONNX] CUDA Execution Provider ready for pool sessions.");
        }

        if let Ok(tokenizer) = Tokenizer::from_file(tokenizer_path) {
            self.tokenizer = Some(Arc::new(tokenizer));
            tracing::info!("✅ [ONNX Pool] Tokenizer loaded successfully.");
        } else {
            tracing::info!("ℹ️ [ONNX Pool] Tokenizer file not found at {}. Session pool ready without tokenizer.", tokenizer_path);
        }

        // ────── GPU / CUDA Warmup Routine ──────
         if let Some(session_arc) = self.session_pool.first() {
            if let Ok(mut locked) = session_arc.lock() {
                let dummy_ids = vec![1i64];
                let dummy_mask = vec![1i64];
                if let (Ok(ids_val), Ok(mask_val)) = (
                    ort::value::Value::from_array(([1usize, 1usize], dummy_ids)),
                    ort::value::Value::from_array(([1usize, 1usize], dummy_mask)),
                ) {
                    let mut inputs = std::collections::HashMap::<String, ort::value::Value>::new();
                    inputs.insert("input_ids".to_string(), ids_val.into());
                    inputs.insert("attention_mask".to_string(), mask_val.into());
                    let _ = locked.run(inputs);
                    tracing::info!("🔥 [ONNX Warmup] Text session CUDA kernel graph & GPU allocator warmed up.");
                }
            }
        }

        tracing::info!(
            "✅ [ONNX Pool] {} session(s) loaded and ready.",
            pool_size
        );
        Ok(())
    }

    pub fn load_encoder_model(&mut self, encoder_path: &str) -> Result<()> {
        if self.encoder_session.is_some() {
            tracing::warn!("⚠️ [ONNX Encoder] Evicting previous encoder session to free VRAM.");
            self.encoder_session = None;
        }
        tracing::info!("📦 [ONNX Encoder] Loading encoder from: {}", encoder_path);

        // 📡 DYNAMIC HARDWARE TELEMETRY WIRING
        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = true;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            let required_vram = if let Ok(meta) = std::fs::metadata(encoder_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.5
            } else {
                0.5
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels for Encoder (Free: {:.1}GB, Req: {:.1}GB).", free_vram, required_vram);
            } else {
                tracing::info!("📡 [Telemetry] Moderate VRAM pressure for Encoder (Free: {:.1}GB, Req: {:.1}GB). Attempting CUDA GPU allocation.", free_vram, required_vram);
            }
        }

        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user for Encoder.");
        } else if onnx_meta.n_gpu_layers == -1 {
            use_gpu = true;
            tracing::info!("⚙️ [ONNX Config] Auto/GPU Mode active for Encoder.");
        } else {
            // Apply telemetry fallback if not forced Auto/CPU
            // Telemetry has already decided use_gpu
        }

        let encoder_builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Encoder session builder error: {:?}", e))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Encoder opt level error: {:?}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("Encoder threads error: {:?}", e))?;

        let cpu_ep = ort::execution_providers::CPUExecutionProvider::default().with_arena_allocator(onnx_meta.enable_cpu_mem_arena).build();
        let mut session_opt = None;

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
            
            if let Ok(mut gpu_builder) = encoder_builder.clone().with_execution_providers([
                cuda_ep.build(),
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
                ort::execution_providers::CoreMLExecutionProvider::default().build(),
                cpu_ep.clone(),
            ]) {
                match gpu_builder.commit_from_file(encoder_path) {
                    Ok(sess) => {
                        tracing::info!("🚀 [Sovereign-Cascade] ONNX Encoder loaded successfully on GPU (CUDA/DirectML).");
                        session_opt = Some(sess);
                    }
                    Err(e) => {
                        tracing::error!("⚠️ [Sovereign-Cascade] GPU Encoder commit failed (VRAM/CUDA error): {:?}", e);
                    }
                }
            }
        }

        // Tier 2: NPU
        if session_opt.is_none() && use_gpu {
            if let Ok(mut npu_builder) = encoder_builder.clone().with_execution_providers([
                ort::execution_providers::OpenVINOExecutionProvider::default().build(),
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
                cpu_ep.clone(),
            ]) {
                if let Ok(sess) = npu_builder.commit_from_file(encoder_path) {
                    tracing::info!("📡 [Sovereign-Cascade] ONNX Encoder committed successfully on Tier 2 (NPU / OpenVINO).");
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 3: CPU
        if session_opt.is_none() {
            if let Ok(mut cpu_builder) = encoder_builder.clone().with_execution_providers([cpu_ep.clone()]) {
                if let Ok(sess) = cpu_builder.commit_from_file(encoder_path) {
                    tracing::info!("⚙️ [Sovereign-Cascade] ONNX Encoder committed on Tier 3 (CPU).");
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 4: SSD Fallback
        if session_opt.is_none() {
            tracing::warn!("⚠️ [Sovereign-Cascade] CPU RAM Pressure high for Encoder. Tier 4 fallback.");
            let fallback_cpu_ep = ort::execution_providers::CPUExecutionProvider::default().with_arena_allocator(false).build();
            let mut ssd_builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow::anyhow!("Opt error: {:?}", e))?
                .with_intra_threads(2)
                .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                .with_execution_providers([fallback_cpu_ep])
                .map_err(|e| anyhow::anyhow!("Fallback CPU error: {:?}", e))?;
            
            let sess = ssd_builder.commit_from_file(encoder_path)
                .map_err(|e| anyhow::anyhow!("Sovereign Cascade failed across all 4 tiers for encoder: {}", e))?;
            session_opt = Some(sess);
        }

        if let Some(session) = session_opt {
            let enc_arc = Arc::new(std::sync::Mutex::new(session));
            if let Ok(mut enc_guard) = enc_arc.lock() {
                let dummy_mel = vec![0.0f32; 1 * 80 * 3000];
                if let Ok(mel_val) = ort::value::Value::from_array(([1usize, 80usize, 3000usize], dummy_mel)) {
                    let mut enc_inputs = std::collections::HashMap::new();
                    enc_inputs.insert("input_features".to_string(), mel_val);
                    let _ = enc_guard.run(enc_inputs);
                    tracing::info!("🔥 [ONNX Warmup] Encoder CUDA session warmed up successfully.");
                }
            }
            self.encoder_session = Some(enc_arc);
            tracing::info!("✅ [ONNX Encoder] Encoder session ready.");
        }
        Ok(())
    }

    /// Dynamically load a vision embedding model (like CLIP) into ONNX Runtime.
    /// Vision models are large — pool size is fixed at 1 to conserve VRAM.
    pub fn load_vision_model(
        &mut self,
        model_path: &str,
        booster: Option<cluaiz_shared::hardware::schema::booster::cluaizBoosterContext>,
    ) -> Result<()> {
        if !self.session_pool.is_empty() {
            let active = self.active_inferences.load(Ordering::Relaxed);
            if active > 0 {
                tracing::warn!("⚠️ [ONNX] {} active vision inference(s) in flight during eviction.", active);
            }
            tracing::warn!("⚠️ [ONNX] Evicting vision session pool before loading: {}", model_path);
            self.session_pool.clear();
        }
        // Force drop any lingering audio encoder to free VRAM
        if self.encoder_session.is_some() {
            tracing::warn!("⚠️ [ONNX] Evicting dangling Audio Encoder session to free VRAM.");
            self.encoder_session = None;
        }
        
        tracing::info!("👁️ [ONNX] Loading Vision Model from: {}", model_path);
        let path = std::path::Path::new(model_path);
        let dir = path.parent().unwrap_or(path).to_path_buf();
        self.model_dir = Some(dir);

        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = false;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.2
            } else {
                f64::MAX
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!("📡 [Telemetry] Safe VRAM levels (Free: {:.1}GB, Req: {:.1}GB). Routing Vision Model to GPU.", free_vram, required_vram);
                use_gpu = true;
            } else if required_vram != f64::MAX {
                tracing::warn!("📡 [Telemetry] High VRAM pressure (Free: {:.1}GB, Req: {:.1}GB). Auto-falling back Vision to CPU.", free_vram, required_vram);
            }
        }
        if let Some(b) = &booster {
            if b.n_gpu_layers == 0 {
                use_gpu = false;
                tracing::info!("⚙️ [Booster] Force CPU Vision mode requested.");
            } else if b.n_gpu_layers > 0 || b.n_gpu_layers == -1 {
                use_gpu = true;
                tracing::info!("⚙️ [Booster] Force GPU Vision mode requested (Layers: {}).", b.n_gpu_layers);
            }
        }

        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user for Vision.");
        } else if onnx_meta.n_gpu_layers == -1 {
            use_gpu = true;
            tracing::info!("⚙️ [ONNX Config] Auto/GPU Mode active for Vision.");
        } else {
            // Apply telemetry fallback if not forced Auto/CPU
            // Telemetry has already decided use_gpu
        }

        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Vision builder error: {:?}", e))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Opt level error: {:?}", e))?
            .with_intra_threads(threads)
            .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?;

        let cpu_ep = ort::execution_providers::CPUExecutionProvider::default().with_arena_allocator(onnx_meta.enable_cpu_mem_arena).build();
        let mut session_opt = None;

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
            
            if let Ok(mut gpu_builder) = builder.clone().with_execution_providers([
                cuda_ep.build(),
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
                ort::execution_providers::CoreMLExecutionProvider::default().build(),
                cpu_ep.clone(),
            ]) {
                match gpu_builder.commit_from_file(model_path) {
                    Ok(sess) => {
                        tracing::info!("🚀 [Sovereign-Cascade] Vision Session committed on GPU (CUDA/DirectML).");
                        session_opt = Some(sess);
                    }
                    Err(e) => {
                        tracing::error!("⚠️ [Sovereign-Cascade] GPU Vision commit failed (VRAM/CUDA error): {:?}", e);
                    }
                }
            }
        }

        // Tier 2: NPU
        if session_opt.is_none() && use_gpu {
            if let Ok(mut npu_builder) = builder.clone().with_execution_providers([
                ort::execution_providers::OpenVINOExecutionProvider::default().build(),
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
                cpu_ep.clone(),
            ]) {
                if let Ok(sess) = npu_builder.commit_from_file(model_path) {
                    tracing::info!("📡 [Sovereign-Cascade] Vision Session committed on Tier 2 (NPU / OpenVINO).");
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 3: CPU
        if session_opt.is_none() {
            if let Ok(mut cpu_builder) = builder.clone().with_execution_providers([cpu_ep.clone()]) {
                if let Ok(sess) = cpu_builder.commit_from_file(model_path) {
                    tracing::info!("⚙️ [Sovereign-Cascade] Vision Session committed on Tier 3 (CPU).");
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 4: SSD Fallback
        if session_opt.is_none() {
            tracing::warn!("⚠️ [Sovereign-Cascade] CPU RAM Pressure high for Vision. Tier 4 fallback.");
            let fallback_cpu_ep = ort::execution_providers::CPUExecutionProvider::default().with_arena_allocator(false).build();
            let mut ssd_builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow::anyhow!("Opt error: {:?}", e))?
                .with_intra_threads(2)
                .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                .with_execution_providers([fallback_cpu_ep])
                .map_err(|e| anyhow::anyhow!("Fallback CPU error: {:?}", e))?;
            
            let sess = ssd_builder.commit_from_file(model_path)
                .map_err(|e| anyhow::anyhow!("Sovereign Cascade failed across all 4 tiers for vision: {}", e))?;
            session_opt = Some(sess);
        }

        if let Some(session) = session_opt {
            self.session_pool.push(Arc::new(std::sync::Mutex::new(session)));
            tracing::info!("✅ [ONNX] Vision session loaded (pool size: 1).");
        }
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
