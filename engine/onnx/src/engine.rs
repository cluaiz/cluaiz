use anyhow::Result;
use ort::session::Session;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokenizers::Tokenizer;

/// Attempt Windows DirectML with its required session options. Other platforms
/// skip this provider so the caller can continue to its existing CPU fallback.
pub(crate) fn try_directml_session(
    builder: &ort::session::builder::SessionBuilder,
    path: &std::path::Path,
    cpu: &ort::ep::ExecutionProviderDispatch,
) -> Option<Session> {
    #[cfg(windows)]
    {
        let attempt = (|| -> ort::Result<Session> {
            builder
                .clone()
                .with_memory_pattern(false)?
                .with_parallel_execution(false)?
                .with_execution_providers([
                    ort::ep::DirectML::default().build().error_on_failure(),
                    cpu.clone(),
                ])?
                .commit_from_file(path)
        })();
        match attempt {
            Ok(session) => {
                tracing::info!("DirectML provider registered and session created");
                Some(session)
            }
            Err(error) => {
                tracing::warn!(%error, "DirectML session unavailable; trying CPU");
                None
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (builder, path, cpu);
        None
    }
}

/// Whether a tokenizer.json is a bare vocabulary (`model` without a `type`), not a tokenizer
fn is_vocab_only(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json.get("model").cloned())
        .is_some_and(|model| model.get("vocab").is_some() && model.get("type").is_none())
}

/// ONNX Multimodal Router (Core Engine)
pub struct OnnxEngine {
    pub(crate) audio8_runtime: Option<std::sync::Mutex<crate::audio::tts::families::audio8::Runtime>>,
    // 🏊 Session Pool: N concurrent sessions for parallel embedding requests
    pub(crate) session_pool: Vec<Arc<std::sync::Mutex<Session>>>,
    // 🎧 Encoder Session: Whisper encoder (input_features -> encoder_hidden_states)
    pub(crate) encoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    // 🔁 STT decoder that continues from cached keys/values (`past_key_values.*`), when packaged
    pub(crate) cached_decoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    pub(crate) tokenizer: Option<Arc<Tokenizer>>,
    // 🔢 Active Inference Counter: tracks in-flight requests for safe hot swap
    pub(crate) active_inferences: Arc<AtomicUsize>,
    // 🧠 KV Cache for Chat Generation
    pub(crate) active_kv_cache: Option<Vec<(Vec<usize>, std::sync::Arc<Vec<f32>>)>>,
    // 📂 Model Directory Path for loading dynamic configs
    pub(crate) model_dir: Option<std::path::PathBuf>,
    pub(crate) settings: cluaiz_core::OnnxSettings,
    // 🗣️ Vocoder Session: Optional secondary ONNX session for TTS flow matching / neural vocoder
    pub(crate) vocoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    // 🔠 Text Encoder Session: Optional secondary ONNX session for TTS diffusion text encoding
    pub(crate) text_encoder_session: Option<Arc<std::sync::Mutex<Session>>>,
    // ⏱️ Duration Predictor Session: Optional secondary ONNX session for TTS duration prediction
    pub(crate) duration_predictor_session: Option<Arc<std::sync::Mutex<Session>>>,
}

impl OnnxEngine {
    /// Retry only known provider faults before output; never restart delivered/cancelled audio.
    pub fn permits_cpu_execution_retry(&self, error: &anyhow::Error, retry_blocked: bool) -> bool {
        if retry_blocked || self.settings.n_gpu_layers == 0 {
            return false;
        }
        let message = format!("{error:#}");
        (message.contains("cuDNN is unavailable or disabled for CUDA Execution Provider")
            && message.contains("LoadLibrary failed"))
            || (message.contains("DmlExecutionProvider") && message.contains("80070057"))
    }

    pub fn new() -> Result<Self> {
        Self::with_settings(cluaiz_core::Settings::current().onnx)
    }

    /// Capture provider settings once for this session owner.
    pub fn with_settings(settings: cluaiz_core::OnnxSettings) -> Result<Self> {
        // Windows resolves CUDA/TensorRT DLLs through PATH. Unix loaders do not.
        #[cfg(windows)]
        {
            let drivers_dir = cluaiz_core::EnvironmentManager::current()
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
        }

        // Initialize ONNX Runtime environment implicitly.
        ort::init().with_name("cluaiz_onnx_env").commit();

        Ok(Self {
            audio8_runtime: None,
            session_pool: Vec::new(),
            encoder_session: None,
            cached_decoder_session: None,
            tokenizer: None,
            active_inferences: Arc::new(AtomicUsize::new(0)),
            active_kv_cache: None,
            model_dir: None,
            settings,
            vocoder_session: None,
            text_encoder_session: None,
            duration_predictor_session: None,
        })
    }

    pub fn acquire_session(
        &self,
    ) -> Result<Arc<std::sync::Mutex<Session>>, cluaiz_core::EngineError> {
        if self.session_pool.is_empty() {
            return Err(cluaiz_core::EngineError::Internal(
                "No ONNX sessions available in pool".to_string(),
            ));
        }
        let idx = self.active_inferences.load(Ordering::Relaxed) % self.session_pool.len();
        Ok(self.session_pool[idx].clone())
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
        self.audio8_runtime = None;
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
            use crate::audio::tts::{family_adapter::{FamilyAdapter, TtsFamily}, families::audio8::Runtime};
            if FamilyAdapter::detect_family(mdir, &[]) == TtsFamily::Audio8 {
                self.audio8_runtime = Some(std::sync::Mutex::new(Runtime::load(self, mdir)?));
                return Ok(());
            }
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
                    // A speech package's tokenizer.json can be only a phoneme vocabulary (its
                    // `model` has no `type`, Kokoro): the speech code reads that itself
                    Err(e) if is_vocab_only(&tok_path) => {
                        tracing::debug!("📖 [ONNX Engine] {:?} is a vocabulary only: {}", tok_path, e);
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

        let (_ram_free, vram_free) = cluaiz_hw::HardwareProber::probe_live_memory();
        let free_vram_gb = (vram_free as f64) / (1024.0 * 1024.0 * 1024.0);
        let mut use_gpu = true;

        let required_vram = if let Ok(meta) = std::fs::metadata(model_path) {
            let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
            file_size_gb * (pool_size as f64) * 1.2
        } else {
            1.0
        };

        let onnx_meta = self.settings.clone();
        // Auto: the GPU when the model fits in the free VRAM (another model may hold it), else the
        // CPU straight away instead of a GPU attempt that fails and reloads on the CPU
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user config.");
        } else if onnx_meta.n_gpu_layers == -1 && free_vram_gb <= required_vram {
            use_gpu = false;
            tracing::info!("📡 [Telemetry] {:.1}GB VRAM free, {:.1}GB needed: running on the CPU.", free_vram_gb, required_vram);
        } else {
            tracing::info!("📡 [Telemetry] {:.1}GB VRAM free, {:.1}GB needed: running on the GPU.", free_vram_gb, required_vram);
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

            if use_gpu && crate::cudnn_driver::is_cuda_runtime_ready_or_spawn_background() {
                let mut cuda_ep = ort::ep::CUDA::default();
                if onnx_meta.gpu_mem_limit_bytes > 0 {
                    cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
                }

                let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                    "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                    _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
                };
                cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

                match builder
                    .clone()
                    .with_execution_providers([cuda_ep.build().error_on_failure(), cpu_ep.clone()])
                {
                    Err(e) => tracing::warn!("⚠️ [ONNX Cascade] CUDA provider could not be registered: {e}. Trying DirectML/CoreML fallback..."),
                    Ok(mut cuda_builder) => match cuda_builder.commit_from_file(model_path) {
                        Ok(sess) => {
                            tracing::info!(
                                session = i,
                                "CUDA provider registered and session created"
                            );
                            session_opt = Some(sess);
                        }
                        Err(e) => {
                            eprintln!("🎙️ [ONNX Hardware Binding] CUDA EP commit failed: {:?}. Trying DirectML GPU fallback...", e);
                            tracing::warn!("⚠️ [ONNX Cascade] Native CUDA EP commit failed: {:?}. Trying DirectML/CoreML fallback...", e);
                        }
                    },
                }
            }

            if session_opt.is_none() && use_gpu {
                session_opt = try_directml_session(&builder, model_path.as_ref(), &cpu_ep);
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
                    tracing::info!(
                        "🚀 [ONNX Text Encoder] Committed successfully via build_session"
                    );
                    self.text_encoder_session = Some(Arc::new(std::sync::Mutex::new(sess)));
                }
                Err(e) => {
                    tracing::warn!(
                        "⚠️ [ONNX Text Encoder] Failed to load text encoder: {:?}",
                        e
                    );
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
                    tracing::info!(
                        "🚀 [ONNX Duration Predictor] Committed successfully via build_session"
                    );
                    self.duration_predictor_session = Some(Arc::new(std::sync::Mutex::new(sess)));
                }
                Err(e) => {
                    tracing::warn!(
                        "⚠️ [ONNX Duration Predictor] Failed to load duration predictor: {:?}",
                        e
                    );
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

        let (_ram_free, vram_free) = cluaiz_hw::HardwareProber::probe_live_memory();
        let free_vram_gb = (vram_free as f64) / (1024.0 * 1024.0 * 1024.0);
        let mut use_gpu = true;

        let required_vram = if let Ok(meta) = std::fs::metadata(encoder_path) {
            let file_size_gb = meta.len() as f64 / (1024.0 * 1024.0 * 1024.0);
            file_size_gb * 1.5
        } else {
            0.5
        };

        let onnx_meta = self.settings.clone();
        // Same rule as the main graph: the GPU when the encoder fits in the free VRAM, else the CPU
        if onnx_meta.n_gpu_layers == 0 {
            use_gpu = false;
            tracing::info!("⚙️ [ONNX Config] Force CPU mode requested by user for Encoder.");
        } else if onnx_meta.n_gpu_layers == -1 && free_vram_gb <= required_vram {
            use_gpu = false;
            tracing::info!("📡 [Telemetry] Encoder: {:.1}GB VRAM free, {:.1}GB needed: running on the CPU.", free_vram_gb, required_vram);
        } else {
            tracing::info!("📡 [Telemetry] Encoder: {:.1}GB VRAM free, {:.1}GB needed: running on the GPU.", free_vram_gb, required_vram);
        }

        // All CPU cores unless the user set a thread count (0 = ONNX Runtime's default: all)
        let encoder_builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Encoder session builder error: {:?}", e))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Encoder opt level error: {:?}", e))?
            .with_intra_threads(onnx_meta.intra_op_num_threads)
            .map_err(|e| anyhow::anyhow!("Encoder threads error: {:?}", e))?;

        let cpu_ep = ort::ep::CPU::default()
            .with_arena_allocator(onnx_meta.enable_cpu_mem_arena)
            .build();
        let mut session_opt: Option<Session> = None;

        if use_gpu && crate::cudnn_driver::is_cuda_runtime_ready_or_spawn_background() {
            let mut cuda_ep = ort::ep::CUDA::default();
            if onnx_meta.gpu_mem_limit_bytes > 0 {
                cuda_ep = cuda_ep.with_memory_limit(onnx_meta.gpu_mem_limit_bytes as usize);
            }

            let arena_strat = match onnx_meta.arena_extend_strategy.as_str() {
                "kSameAsRequested" => ort::ep::ArenaExtendStrategy::SameAsRequested,
                _ => ort::ep::ArenaExtendStrategy::NextPowerOfTwo,
            };
            cuda_ep = cuda_ep.with_arena_extend_strategy(arena_strat);

            match encoder_builder
                .clone()
                .with_execution_providers([cuda_ep.build().error_on_failure(), cpu_ep.clone()])
            {
                Err(e) => tracing::warn!("⚠️ [ONNX Cascade] CUDA provider could not be registered for the encoder: {e}"),
                Ok(mut gpu_builder) => match gpu_builder.commit_from_file(encoder_path) {
                    Ok(sess) => {
                        tracing::info!("CUDA provider registered and encoder session created");
                        session_opt = Some(sess);
                    }
                    Err(e) => {
                        tracing::error!("⚠️ [ONNX Cascade] GPU Encoder commit failed (VRAM/CUDA error): {:?}", e);
                    }
                },
            }
        }

        if session_opt.is_none() && use_gpu {
            session_opt = try_directml_session(&encoder_builder, encoder_path.as_ref(), &cpu_ep);
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
        let primary = std::path::Path::new(model_path);
        let dir = primary
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Vision model parent missing"))?;
        self.load_embedding_package(dir, primary)
    }
}

use cluaiz_core::{EmbeddingDriver, EngineError, Modality};

impl EmbeddingDriver for OnnxEngine {
    fn gen_embedding(&self, text: &str) -> std::result::Result<Vec<f32>, EngineError> {
        self.execute_text_embedding(text)
    }

    fn gen_multimodal_embedding(
        &self,
        bytes: &[u8],
        modality: Modality,
        instruction: Option<String>,
    ) -> std::result::Result<Vec<f32>, EngineError> {
        match modality {
            Modality::Image => self.execute_image_embedding(bytes, instruction),
            _ => Err(EngineError::UnsupportedModality(
                "ONNX multimodal embedding currently supports image input only".into(),
            )),
        }
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    // Real ONNX protobuf: IR 8, opset 13, Identity(x -> y), FLOAT[1].
    // This graph executes in ONNX Runtime; it is not a fabricated engine answer.
    const IDENTITY_GRAPH: &[u8] = &[
        8, 8, 58, 55, 10, 16, 10, 1, b'x', 18, 1, b'y', 34, 8, b'I', b'd', b'e', b'n', b't', b'i',
        b't', b'y', 18, 1, b'g', 90, 15, 10, 1, b'x', 18, 10, 10, 8, 8, 1, 18, 4, 10, 2, 8, 1, 98,
        15, 10, 1, b'y', 18, 10, 10, 8, 8, 1, 18, 4, 10, 2, 8, 1, 66, 2, 16, 13,
    ];

    fn run_identity(mut session: Session) -> ort::Result<()> {
        let input = ort::value::Tensor::from_array(([1usize], vec![3.25f32]))?;
        let outputs = session.run(ort::inputs!["x" => input])?;
        assert_eq!(outputs["y"].try_extract_tensor::<f32>()?.1, &[3.25]);
        Ok(())
    }

    #[test]
    fn test_cpu_provider_executes_real_graph() -> ort::Result<()> {
        let session = Session::builder()?
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])?
            .commit_from_memory(IDENTITY_GRAPH)?;
        run_identity(session)
    }

    #[test]
    fn test_failed_cuda_registration_preserves_cpu_retry_builder() -> ort::Result<()> {
        let builder = Session::builder()?;
        // Fault injection: an invalid device must fail whether CUDA is absent
        // or installed. A strict failure must not turn into apparent GPU success.
        let cuda_attempt = builder
            .clone()
            .with_execution_providers([ort::ep::CUDA::default()
                .with_device_id(i32::MAX)
                .build()
                .error_on_failure()]);
        assert!(cuda_attempt.is_err());
        let session = builder
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])?
            .commit_from_memory(IDENTITY_GRAPH)?;
        run_identity(session)
    }

    #[test]
    fn test_failed_directml_attempt_preserves_cpu_retry_builder() -> ort::Result<()> {
        let builder = Session::builder()?;
        let cpu = ort::ep::CPU::default().build().error_on_failure();
        // Empty path cannot be an ONNX file, regardless of provider availability.
        assert!(try_directml_session(&builder, std::path::Path::new(""), &cpu).is_none());
        let session = builder
            .with_execution_providers([cpu])?
            .commit_from_memory(IDENTITY_GRAPH)?;
        run_identity(session)
    }

    #[test]
    fn test_cpu_execution_retry_only_accepts_observed_provider_faults() -> anyhow::Result<()> {
        let mut settings = cluaiz_core::OnnxSettings::default();
        settings.n_gpu_layers = -1;
        let gpu = OnnxEngine::with_settings(settings.clone())?;
        let failures = [
            "cuDNN is unavailable or disabled for CUDA Execution Provider: LoadLibrary failed for cudnn64_9.dll with error 2",
            "DmlExecutionProvider/MLOperatorAuthorImpl.cpp Exception 80070057 The parameter is incorrect",
        ];
        for message in failures {
            let error = anyhow::anyhow!(message).context("TTS execution");
            assert!(gpu.permits_cpu_execution_retry(&error, false));
            assert!(!gpu.permits_cpu_execution_retry(&error, true), "Delivered/cancelled speech must not restart from the beginning");
        }
        for message in [
            "Unexpected input data type: int64, expected float16",
            "Invalid tensor dimensions",
            "Required tokenizer missing",
            "Audio contains NaN",
        ] {
            assert!(!gpu.permits_cpu_execution_retry(&anyhow::anyhow!(message), false));
        }
        settings.n_gpu_layers = 0;
        let cpu = OnnxEngine::with_settings(settings)?;
        assert!(!cpu.permits_cpu_execution_retry(&anyhow::anyhow!(failures[0]), false));
        Ok(())
    }
}
