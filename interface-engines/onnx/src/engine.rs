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
    // 🗣️ Vocoder Session: Optional secondary ONNX session for TTS flow matching / neural vocoder
    pub(crate) vocoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    // 🔠 Text Encoder Session: Optional secondary ONNX session for TTS diffusion text encoding
    pub(crate) text_encoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    // ⏱️ Duration Predictor Session: Optional secondary ONNX session for TTS duration prediction
    pub(crate) duration_predictor_session: Option<Arc<std::sync::Mutex<Session>>>,
}

impl OnnxEngine {
    pub fn new() -> Result<Self> {
        // Prepend drivers directory to PATH so ONNX Runtime loads CUDA/TensorRT DLLs
        let drivers_dir = cluaiz_shared::environment::EnvironmentManager::current()
            .engine_dir()
            .join("drivers");
        if drivers_dir.exists() {
            let current_path = std::env::var_os("PATH").unwrap_or_default();
            let mut new_path = drivers_dir.into_os_string();
            if !current_path.is_empty() {
                new_path.push(";");
                new_path.push(current_path);
            }
            std::env::set_var("PATH", new_path);
        }

        // Initialize ONNX Runtime environment implicitly.
        ort::init().with_name("cluaiz_onnx_env").commit();

        Ok(Self {
            session_pool: Vec::new(),
            encoder_session: None,
            tokenizer: None,
            active_inferences: Arc::new(AtomicUsize::new(0)),
            active_kv_cache: None,
            model_dir: None,
            vocoder_session: None,
            text_encoder_session: None,
            duration_predictor_session: None,
        })
    }

    pub fn acquire_session(
        &self,
    ) -> Result<Arc<std::sync::Mutex<Session>>, neural_core::interfaces::router_contract::EngineError>
    {
        if self.session_pool.is_empty() {
            return Err(
                neural_core::interfaces::router_contract::EngineError::Internal(
                    "No ONNX sessions available in pool".to_string(),
                ),
            );
        }
        let idx = self.active_inferences.load(Ordering::Relaxed) % self.session_pool.len();
        Ok(self.session_pool[idx].clone())
    }

    pub fn build_session(&self, path: &std::path::Path) -> Result<Session> {
        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        
        let model_size_gb = std::fs::metadata(path)
            .map(|m| (m.len() as f64) / (1024.0 * 1024.0 * 1024.0))
            .unwrap_or(0.5);

        let req = cluaiz_shared::hardware::ResourceRequest {
            engine_type: cluaiz_shared::hardware::EngineType::ONNX,
            inference_mode: cluaiz_shared::hardware::InferenceMode::Embedding,
            model_size_gb,
            model_path: path.to_path_buf(),
        };

        let grant = cluaiz_shared::hardware::negotiate_resource(&req).ok();
        let use_gpu = grant
            .as_ref()
            .map(|g| g.tier != cluaiz_shared::hardware::PlacementTier::CpuOnly)
            .unwrap_or(onnx_meta.n_gpu_layers != 0);

        let mut builder = Session::builder()?;
        let cpu_ep = ort::ep::CPU::default().with_arena_allocator(onnx_meta.enable_cpu_mem_arena).build();
        if use_gpu {
            let mut cuda_ep = ort::ep::CUDA::default();
            let mem_limit = grant.as_ref()
                .map(|g| (g.vram_budget_gb * 1024.0 * 1024.0 * 1024.0) as usize)
                .filter(|&l| l > 0)
                .unwrap_or(onnx_meta.gpu_mem_limit_bytes);
            if mem_limit > 0 {
                cuda_ep = cuda_ep.with_memory_limit(mem_limit);
            }
            let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
            };
            cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

            builder = builder
                .with_execution_providers([cuda_ep.build(), cpu_ep])
                .map_err(|e| anyhow::anyhow!("GPU EP error: {:?}", e))?;
        } else {
            builder = builder
                .with_execution_providers([cpu_ep])
                .map_err(|e| anyhow::anyhow!("CPU EP error: {:?}", e))?;
        }
        let session = builder.commit_from_file(path)?;
        Ok(session)
    }

    pub(crate) fn scan_onnx_files_recursive(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut onnx_files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    onnx_files.extend(Self::scan_onnx_files_recursive(&path));
                } else if let Some(ext) = path.extension() {
                    if ext == "onnx" {
                        onnx_files.push(path);
                    }
                }
            }
        }
        onnx_files
    }

    pub fn load_model(&mut self, model_path: &str, pool_size: usize) -> Result<()> {
        tracing::info!(
            "📦 [ONNX Engine] Loading primary model from: {} (pool_size: {})",
            model_path,
            pool_size
        );

        self.session_pool.clear();
        self.vocoder_session = None;
        self.text_encoder_session = None;
        self.duration_predictor_session = None;

        if let Ok(meta_dir) = std::fs::metadata(model_path) {
            if meta_dir.is_dir() {
                self.model_dir = Some(std::path::PathBuf::from(model_path));
            } else if let Some(parent) = std::path::Path::new(model_path).parent() {
                self.model_dir = Some(parent.to_path_buf());
            }
        }

        if let Some(ref mdir) = self.model_dir {
            let tok_path = mdir.join("tokenizer.json");
            if tok_path.exists() {
                match Tokenizer::from_file(&tok_path) {
                    Ok(tok) => {
                        tracing::info!(
                            "📖 [ONNX Engine] Successfully loaded Tokenizer from {:?}",
                            tok_path
                        );
                        self.tokenizer = Some(Arc::new(tok));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "⚠️ [ONNX Engine] Failed to load tokenizer.json from {:?}: {}",
                            tok_path,
                            e
                        );
                    }
                }
            }
        }

        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = true;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * (pool_size as f64) * 1.2
            } else {
                1.0
            };

            if free_vram > required_vram && state.vram_pressure_pct < 90 {
                tracing::info!(
                    "📡 [Telemetry] High VRAM available ({:.1}GB free, {:.1}GB req). GPU active.",
                    free_vram,
                    required_vram
                );
            } else {
                tracing::info!("📡 [Telemetry] Moderate VRAM pressure ({:.1}GB free, {:.1}GB req). Attempting CUDA GPU allocation.", free_vram, required_vram);
            }
        }

        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user config.");
        } else if onnx_meta.n_gpu_layers == -1 {
            use_gpu = true;
            tracing::info!("⚙️ [ONNX Config] Auto/GPU Mode active.");
        }

        for i in 0..pool_size {
            let mut builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
                .map_err(|e| anyhow::anyhow!("Opt level error: {:?}", e))?
                .with_intra_threads(onnx_meta.intra_op_num_threads)
                .map_err(|e| anyhow::anyhow!("Intra threads error: {:?}", e))?;

            builder = builder
                .with_deterministic_compute(onnx_meta.use_deterministic_compute)
                .map_err(|e| anyhow::anyhow!("Deterministic compute error: {:?}", e))?;

            let cpu_ep = ort::ep::CPU::default()
                .with_arena_allocator(onnx_meta.enable_cpu_mem_arena)
                .build();

            let mut session_opt: Option<Session> = None;

            if use_gpu {
                let mut cuda_ep = ort::ep::CUDA::default();
                if onnx_meta.gpu_mem_limit_bytes > 0 {
                    cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
                }

                let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                    "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                    _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
                };
                cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

                if let Ok(mut cuda_builder) = builder
                    .clone()
                    .with_execution_providers([cuda_ep.clone().build(), cpu_ep.clone()])
                {
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

                // Tier 1B: Fallback to DirectML if native CUDA failed
                if session_opt.is_none() {
                    let dml_ep = ort::ep::DirectML::default().with_device_id(0).build();
                    if let Ok(mut gpu_builder) = builder
                        .clone()
                        .with_execution_providers([dml_ep, cpu_ep.clone()])
                    {
                        match gpu_builder.commit_from_file(model_path) {
                            Ok(sess) => {
                                eprintln!("🎙️ [ONNX Hardware Binding] Session committed ON DIRECTX 12 DIRECTML GPU (Device 0)!");
                                tracing::info!("🚀 [Sovereign-Cascade] ONNX Session [{}] committed on DirectML GPU.", i);
                                session_opt = Some(sess);
                            }
                            Err(e) => {
                                eprintln!("🎙️ [ONNX Hardware Binding] DirectML GPU Session commit failed: {:?}", e);
                                tracing::warn!(
                                    "⚠️ [Sovereign-Cascade] DirectML Session commit failed: {:?}",
                                    e
                                );
                            }
                        }
                    }
                }
            }

            // Tier 2: DirectML fallback (if Tier 1 was fully skipped, i.e., CPU-only mode requested)
            if session_opt.is_none() && use_gpu {
                if let Ok(mut npu_builder) = builder.clone().with_execution_providers([
                    ort::ep::DirectML::default().build(),
                    cpu_ep.clone(),
                ]) {
                    if let Ok(sess) = npu_builder.commit_from_file(model_path) {
                        tracing::info!("📡 [Sovereign-Cascade] ONNX Session [{}] committed successfully on Tier 2 (DirectML).", i);
                        session_opt = Some(sess);
                    }
                }
            }

            // Tier 3: CPU Thread Pool Arena
            if session_opt.is_none() {
                if let Ok(mut cpu_builder) =
                    builder.clone().with_execution_providers([cpu_ep.clone()])
                {
                    if let Ok(sess) = cpu_builder.commit_from_file(model_path) {
                        tracing::info!("⚙️ [Sovereign-Cascade] ONNX Session [{}] committed on Tier 3 (CPU Thread Pool).", i);
                        session_opt = Some(sess);
                    }
                }
            }

            // Tier 4: SSD Memory Arena / Minimal RAM Swap (Zero Crash Fallback)
            if session_opt.is_none() {
                tracing::warn!("⚠️ [Sovereign-Cascade] CPU RAM Pressure high. Falling back to Tier 4 (SSD Memory Arena / Low-RAM Swap).");
                let fallback_cpu_ep = ort::ep::CPU::default().with_arena_allocator(false).build();
                let mut ssd_builder = Session::builder()
                    .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                    .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                    .map_err(|e| anyhow::anyhow!("Opt error: {:?}", e))?
                    .with_intra_threads(2)
                    .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                    .with_execution_providers([fallback_cpu_ep])
                    .map_err(|e| anyhow::anyhow!("Fallback CPU error: {:?}", e))?;

                let sess = ssd_builder.commit_from_file(model_path).map_err(|e| {
                    anyhow::anyhow!("Sovereign Cascade failed across all 4 tiers: {}", e)
                })?;
                session_opt = Some(sess);
            }

            if let Some(session) = session_opt {
                self.session_pool
                    .push(Arc::new(std::sync::Mutex::new(session)));
            }
        }

        // ────── Recursive Sub-Model Package Scanning ──────
        let scan_dir = if std::path::Path::new(model_path).is_dir() {
            std::path::Path::new(model_path).to_path_buf()
        } else {
            std::path::Path::new(model_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf()
        };
        let all_onnx_files: Vec<std::path::PathBuf> = Self::scan_onnx_files_recursive(&scan_dir);

        // 1. Vocoder Discovery
        let vocoder_file = all_onnx_files
            .iter()
            .find(|p: &&std::path::PathBuf| {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                name.contains("vocoder")
                    || name.contains("hift")
                    || name.contains("generator")
                    || name.contains("melgan")
                    || name.contains("vocos")
            })
            .cloned();

        if let Some(vp) = vocoder_file {
            tracing::info!("🗣️ [ONNX Vocoder] Found Neural Vocoder: {:?}", vp);
            match self.build_session(&vp) {
                Ok(sess) => {
                    tracing::info!("🚀 [ONNX Vocoder] Committed successfully via build_session");
                    self.vocoder_session = Some(Arc::new(std::sync::Mutex::new(sess)));
                }
                Err(e) => {
                    tracing::warn!("⚠️ [ONNX Vocoder] Failed to load vocoder: {:?}", e);
                }
            }
        }

        // 2. Text Encoder Discovery
        let text_enc_file = all_onnx_files
            .iter()
            .find(|p: &&std::path::PathBuf| {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                (name.contains("text_encoder")
                    || name.contains("text_enc")
                    || name.contains("prompt_encoder")
                    || name.contains("speech_tokenizer"))
                    && !name.contains("duration")
            })
            .cloned();

        if let Some(tp) = text_enc_file {
            tracing::info!("🔠 [ONNX Text Encoder] Found Text Encoder: {:?}", tp);
            match self.build_session(&tp) {
                Ok(sess) => {
                    tracing::info!("🚀 [ONNX Text Encoder] Committed successfully via build_session");
                    self.text_encoder_session = Some(Arc::new(std::sync::Mutex::new(sess)));
                }
                Err(e) => {
                    tracing::warn!("⚠️ [ONNX Text Encoder] Failed to load text encoder: {:?}", e);
                }
            }
        }

        // 3. Duration Predictor Discovery
        let dp_file = all_onnx_files
            .iter()
            .find(|p: &&std::path::PathBuf| {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                name.contains("duration_predictor") || name.contains("dp.onnx")
            })
            .cloned();

        if let Some(dp) = dp_file {
            tracing::info!(
                "⏱️ [ONNX Duration Predictor] Found Duration Predictor: {:?}",
                dp
            );
            match self.build_session(&dp) {
                Ok(sess) => {
                    tracing::info!("🚀 [ONNX Duration Predictor] Committed successfully via build_session");
                    self.duration_predictor_session = Some(Arc::new(std::sync::Mutex::new(sess)));
                }
                Err(e) => {
                    tracing::warn!("⚠️ [ONNX Duration Predictor] Failed to load duration predictor: {:?}", e);
                }
            }
        }

        tracing::info!("✅ [ONNX Pool] {} session(s) loaded and ready.", pool_size);

        Ok(())
    }

    pub fn load_encoder_model(&mut self, encoder_path: &str) -> Result<()> {
        if self.encoder_session.is_some() {
            tracing::warn!("⚠️ [ONNX Encoder] Evicting previous encoder session to free VRAM.");
            self.encoder_session = None;
        }
        tracing::info!("📦 [ONNX Encoder] Loading encoder from: {}", encoder_path);

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
                tracing::info!(
                    "📡 [Telemetry] Safe VRAM levels for Encoder (Free: {:.1}GB, Req: {:.1}GB).",
                    free_vram,
                    required_vram
                );
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
        }

        let encoder_builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Encoder session builder error: {:?}", e))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Encoder opt level error: {:?}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("Encoder threads error: {:?}", e))?;

        let cpu_ep = ort::ep::CPU::default()
            .with_arena_allocator(onnx_meta.enable_cpu_mem_arena)
            .build();
        let mut session_opt: Option<Session> = None;

        if use_gpu {
            let mut cuda_ep = ort::ep::CUDA::default();
            if onnx_meta.gpu_mem_limit_bytes > 0 {
                cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
            }

            let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
            };
            cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

            if let Ok(mut gpu_builder) = encoder_builder.clone().with_execution_providers([
                cuda_ep.clone().build(),
                ort::ep::DirectML::default().build(),
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

        // Tier 2: DirectML fallback
        if session_opt.is_none() && use_gpu {
            if let Ok(mut npu_builder) = encoder_builder
                .clone()
                .with_execution_providers([ort::ep::DirectML::default().build(), cpu_ep.clone()])
            {
                if let Ok(sess) = npu_builder.commit_from_file(encoder_path) {
                    tracing::info!(
                        "📡 [Sovereign-Cascade] ONNX Encoder committed on Tier 2 (DirectML)."
                    );
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 3: CPU
        if session_opt.is_none() {
            if let Ok(mut cpu_builder) = encoder_builder
                .clone()
                .with_execution_providers([cpu_ep.clone()])
            {
                if let Ok(sess) = cpu_builder.commit_from_file(encoder_path) {
                    tracing::info!(
                        "⚙️ [Sovereign-Cascade] ONNX Encoder committed on Tier 3 (CPU)."
                    );
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 4: SSD Fallback
        if session_opt.is_none() {
            tracing::warn!(
                "⚠️ [Sovereign-Cascade] CPU RAM Pressure high for Encoder. Tier 4 fallback."
            );
            let fallback_cpu_ep = ort::ep::CPU::default().with_arena_allocator(false).build();
            let mut ssd_builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow::anyhow!("Opt error: {:?}", e))?
                .with_intra_threads(2)
                .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                .with_execution_providers([fallback_cpu_ep])
                .map_err(|e| anyhow::anyhow!("Fallback CPU error: {:?}", e))?;

            let sess = ssd_builder.commit_from_file(encoder_path).map_err(|e| {
                anyhow::anyhow!(
                    "Sovereign Cascade failed across all 4 tiers for encoder: {}",
                    e
                )
            })?;
            session_opt = Some(sess);
        }

        if let Some(session) = session_opt {
            self.encoder_session = Some(Arc::new(std::sync::Mutex::new(session)));
        }

        Ok(())
    }

    pub fn load_vision_model(&mut self, model_path: &str) -> Result<()> {
        tracing::info!("📦 [ONNX Vision] Loading vision model from: {}", model_path);

        let pulse_state = cluaiz_shared::hardware::system_performance::get_pulse();
        let mut use_gpu = true;

        if let Ok(state) = pulse_state.pulse.read() {
            let free_vram = state.vram_total_gb - state.vram_used_gb;
            let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
                let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
                file_size_gb * 1.5
            } else {
                0.5
            };

            if free_vram > required_vram && state.vram_pressure_pct < 95 {
                tracing::info!(
                    "📡 [Telemetry] Safe VRAM levels for Vision (Free: {:.1}GB, Req: {:.1}GB).",
                    free_vram,
                    required_vram
                );
            } else {
                tracing::info!("📡 [Telemetry] Moderate VRAM pressure for Vision (Free: {:.1}GB, Req: {:.1}GB). Attempting CUDA GPU allocation.", free_vram, required_vram);
            }
        }

        let onnx_meta = cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load();
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user for Vision.");
        } else if onnx_meta.n_gpu_layers == -1 {
            use_gpu = true;
            tracing::info!("⚙️ [ONNX Config] Auto/GPU Mode active for Vision.");
        }

        let builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Vision session builder error: {:?}", e))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Vision opt level error: {:?}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("Vision threads error: {:?}", e))?;

        let cpu_ep = ort::ep::CPU::default()
            .with_arena_allocator(onnx_meta.enable_cpu_mem_arena)
            .build();
        let mut session_opt: Option<Session> = None;

        if use_gpu {
            let mut cuda_ep = ort::ep::CUDA::default();
            if onnx_meta.gpu_mem_limit_bytes > 0 {
                cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
            }

            let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
            };
            cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

            if let Ok(mut gpu_builder) = builder.clone().with_execution_providers([
                cuda_ep.clone().build(),
                ort::ep::DirectML::default().build(),
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

        // Tier 2: DirectML fallback
        if session_opt.is_none() && use_gpu {
            if let Ok(mut npu_builder) = builder
                .clone()
                .with_execution_providers([ort::ep::DirectML::default().build(), cpu_ep.clone()])
            {
                if let Ok(sess) = npu_builder.commit_from_file(model_path) {
                    tracing::info!(
                        "📡 [Sovereign-Cascade] Vision Session committed on Tier 2 (DirectML)."
                    );
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 3: CPU
        if session_opt.is_none() {
            if let Ok(mut cpu_builder) = builder.clone().with_execution_providers([cpu_ep.clone()])
            {
                if let Ok(sess) = cpu_builder.commit_from_file(model_path) {
                    tracing::info!(
                        "⚙️ [Sovereign-Cascade] Vision Session committed on Tier 3 (CPU)."
                    );
                    session_opt = Some(sess);
                }
            }
        }

        // Tier 4: SSD Fallback
        if session_opt.is_none() {
            tracing::warn!(
                "⚠️ [Sovereign-Cascade] CPU RAM Pressure high for Vision. Tier 4 fallback."
            );
            let fallback_cpu_ep = ort::ep::CPU::default().with_arena_allocator(false).build();
            let mut ssd_builder = Session::builder()
                .map_err(|e| anyhow::anyhow!("Session builder error: {:?}", e))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow::anyhow!("Opt error: {:?}", e))?
                .with_intra_threads(2)
                .map_err(|e| anyhow::anyhow!("Threads error: {:?}", e))?
                .with_execution_providers([fallback_cpu_ep])
                .map_err(|e| anyhow::anyhow!("Fallback CPU error: {:?}", e))?;

            let sess = ssd_builder.commit_from_file(model_path).map_err(|e| {
                anyhow::anyhow!(
                    "Sovereign Cascade failed across all 4 tiers for vision: {}",
                    e
                )
            })?;
            session_opt = Some(sess);
        }

        if let Some(session) = session_opt {
            self.session_pool
                .push(Arc::new(std::sync::Mutex::new(session)));
            tracing::info!("✅ [ONNX] Vision session loaded (pool size: 1).");
        }
        Ok(())
    }
}

use neural_core::interfaces::router_contract::{EmbeddingDriver, EngineError, Modality};

impl EmbeddingDriver for OnnxEngine {
    fn gen_embedding(&self, text: &str) -> std::result::Result<Vec<f32>, EngineError> {
        self.execute_text_embedding(text)
    }

    fn gen_multimodal_embedding(
        &self,
        _bytes: &[u8],
        _modality: Modality,
        _instruction: Option<String>,
    ) -> std::result::Result<Vec<f32>, EngineError> {
        Err(EngineError::UnsupportedModality(
            "Multimodal embedding not implemented for ONNX text engine".to_string(),
        ))
    }
}
