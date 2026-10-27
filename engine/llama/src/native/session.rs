//! Paired model/projector ownership. Blocking operations belong on runtime workers.

use super::{
    core::NativeLlama,
    media::{CachedChunk, MediaInput, Projector, ProjectorSupport},
};
use anyhow::{bail, Context, Result};
use cluaiz_core::{bytes_to_gib, chat::ChatEnvelope, media::PreparedPart, ActiveUserSettings};
use cluaiz_hw::HardwareProber;
use cluaiz_models::StructuralDNA;
use cluaiz_plan::media_budget::{MediaBudget, MediaRequirements};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectorKey {
    path: PathBuf,
    file_bytes: u64,
    modified: SystemTime,
    threads: i32,
    prefer_gpu: bool,
    weights_bytes: u64,
    workspace_bytes: u64,
    ram_buffer: Option<u64>,
    vram_buffer: Option<u64>,
}

/// Owns a stable text model and a lazily loaded projector. Neither owner is exposed
/// mutably, so model replacement cannot leave the projector pointing at a freed model.
pub struct NativeSession {
    // Rust drops fields in declaration order: projector BEFORE the text model.
    projector: Option<Projector>,
    llama: NativeLlama,
    key: Option<ProjectorKey>,
    budget: Option<MediaBudget>,
    skip_reasoning: Option<Arc<AtomicBool>>,
    /// The running request's stop flag (see `StreamOptions::cancel`)
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<cluaiz_core::processing::ProcessingReporter>,
    /// What the KV cache holds after the last media generation (text tokens and encoded media by
    /// content id), so the next media turn re-encodes only what changed. Empty = nothing reusable.
    media_cache: Vec<CachedChunk>,
}

impl NativeSession {
    pub fn set_processing_progress(
        &mut self,
        progress: Option<cluaiz_core::processing::ProcessingReporter>,
    ) {
        self.progress = progress;
    }
    /// Bound only while this request owns the serialized native generation worker.
    pub fn set_reasoning_control(&mut self, signal: Option<Arc<AtomicBool>>) {
        self.skip_reasoning = signal;
    }

    /// Bound only while this request owns the serialized native generation worker.
    pub fn set_cancel_control(&mut self, signal: Option<Arc<AtomicBool>>) {
        self.cancel = signal;
    }

    /// Loaded context size used by request budgets.
    pub fn context_tokens(&self) -> u32 {
        self.llama.n_ctx
    }

    /// Actual placement after native load fallback.
    pub fn gpu_layers(&self) -> i32 {
        self.llama.n_gpu_layers_loaded
    }

    /// Native timings captured after generation completes.
    pub fn timings(&self) -> super::core::GenerationTimings {
        self.llama.timings()
    }

    /// Thinking report for the most recent generation.
    pub fn thinking_report(&self) -> Option<super::core::ThinkingReport> {
        self.llama.last_thinking.clone()
    }

    /// Context report for the most recent generation.
    pub fn context_report(&self) -> Option<super::core::ContextReport> {
        self.llama.last_context.clone()
    }

    /// The vocabulary, for counting tokens outside the generation lock. Valid while this session
    /// lives: the host clears its copy before dropping the session.
    pub fn vocab(&self) -> super::VocabHandle {
        // SAFETY: the model is loaded for this session's whole life; the call only reads it.
        super::VocabHandle::new(unsafe { crate::ffi::llama_cpp::llama_model_get_vocab(self.llama.model_ptr) })
    }

    /// Native model description without exposing its pointers.
    pub fn describe(&self) -> (String, u64, u64) {
        self.llama.describe()
    }

    /// Parse template tags while retaining the paired model's lifetime.
    pub fn thinking_tags(&self, template: Option<&str>) -> (Option<String>, Option<String>) {
        super::templater::extract_thinking_tags_native(self.llama.model_ptr, template)
    }

    /// Text generation keeps the existing prefill/prefix-cache behavior.
    pub fn stream_text(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        dna: &StructuralDNA,
        cached: &[i32],
        callback: cluaiz_core::contracts::StreamCallback,
    ) -> Result<Vec<i32>> {
        // The text path keeps its own token list of the KV; a media layout no longer describes it
        self.media_cache.clear();
        super::stream::stream_tokens_with_media(
            &mut self.llama,
            prompt,
            max_tokens,
            dna,
            cached,
            callback,
            super::stream::StreamOptions {
                media: None,
                skip_reasoning: self.skip_reasoning.as_deref(),
                cancel: self.cancel.as_deref(),
            },
        )
    }

    /// Saves the KV cache (sequence 0) to `<base>.kv` with what it holds: the text token list, or for a
    /// media chat its chunk layout in `<base>.layout.json` (llama-server's slot save). Returns the bytes.
    pub fn save_memory(&self, base: &Path, tokens: &[i32]) -> Result<u64> {
        let kv = base.with_extension("kv");
        let layout = base.with_extension("layout.json");
        let c_path = CString::new(kv.to_string_lossy().as_bytes())?;
        // SAFETY: live context; the token slice outlives the call, which copies it into the file.
        let written = unsafe {
            crate::ffi::llama_cpp::llama_state_seq_save_file(
                self.llama.ctx_ptr,
                c_path.as_ptr(),
                0,
                tokens.as_ptr(),
                tokens.len(),
            )
        };
        if written == 0 {
            bail!(
                "llama.cpp could not save the chat memory to {}",
                kv.display()
            );
        }
        if self.media_cache.is_empty() {
            let _ = std::fs::remove_file(&layout);
        } else {
            std::fs::write(&layout, serde_json::to_vec(&self.media_cache)?)?;
        }
        // The newest sliding-window checkpoint, so a restored chat can restart before its last answer
        let checkpoint = base.with_extension("ckpt");
        match self.llama.checkpoints.last() {
            Some(c) => {
                let mut bytes = c.n_tokens.to_le_bytes().to_vec();
                bytes.extend_from_slice(&c.data);
                std::fs::write(&checkpoint, bytes)?;
            }
            None => {
                let _ = std::fs::remove_file(&checkpoint);
            }
        }
        Ok(written as u64)
    }

    /// Restores a KV cache saved by `save_memory` into sequence 0 and returns its text token list
    /// (empty for a media chat, whose chunk layout is restored instead). On failure the KV is empty.
    pub fn restore_memory(&mut self, base: &Path) -> Result<Vec<i32>> {
        let kv = base.with_extension("kv");
        let layout = base.with_extension("layout.json");
        let c_path = CString::new(kv.to_string_lossy().as_bytes())?;
        let mut tokens = vec![0_i32; self.llama.n_ctx as usize];
        let mut count = 0_usize;
        // SAFETY: live context; the output buffer holds n_ctx tokens, the most a sequence can have.
        let read = unsafe {
            let memory = crate::ffi::llama_cpp::llama_get_memory(self.llama.ctx_ptr);
            crate::ffi::llama_cpp::llama_memory_seq_rm(memory, 0, -1, -1);
            crate::ffi::llama_cpp::llama_state_seq_load_file(
                self.llama.ctx_ptr,
                c_path.as_ptr(),
                0,
                tokens.as_mut_ptr(),
                tokens.len(),
                &mut count,
            )
        };
        self.media_cache.clear();
        self.llama.checkpoints.clear();
        if read == 0 {
            bail!(
                "llama.cpp could not restore the chat memory from {}",
                kv.display()
            );
        }
        if let Ok(bytes) = std::fs::read(base.with_extension("ckpt")) {
            if bytes.len() > 4 {
                let n_tokens = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                self.llama.checkpoints.push(super::context::Checkpoint {
                    n_tokens,
                    data: bytes[4..].to_vec(),
                });
            }
        }
        tokens.truncate(count);
        if layout.exists() {
            self.media_cache = serde_json::from_slice(&std::fs::read(&layout)?)?;
            tokens.clear();
        }
        Ok(tokens)
    }

    /// Benchmark owns and clears the same model context as generation.
    pub fn bench(
        &mut self,
        prompt: usize,
        generated: usize,
        repetitions: usize,
    ) -> Result<Vec<u64>> {
        self.media_cache.clear();
        self.llama.bench(prompt, generated, repetitions)
    }

    /// Backend-validated asset identity, absent until initialization succeeds.
    pub fn projector_path(&self) -> Option<&Path> {
        self.key.as_ref().map(|key| key.path.as_path())
    }

    /// Takes exclusive ownership of an already initialized model/context.
    pub fn new(llama: NativeLlama) -> Result<Self> {
        if llama.model_ptr.is_null() || llama.ctx_ptr.is_null() {
            bail!("Paired session requires an initialized model/context");
        }
        Ok(Self {
            projector: None,
            llama,
            key: None,
            budget: None,
            skip_reasoning: None,
            cancel: None,
            progress: None,
            media_cache: Vec::new(),
        })
    }

    /// Load/reuse with caller-supplied estimates. File size is a lower bound, not an
    /// estimate of native tensor expansion. Probe live memory only AFTER dropping an old projector.
    pub fn ensure_projector(
        &mut self,
        path: &Path,
        needs: MediaRequirements,
        settings: &ActiveUserSettings,
        prefer_gpu: bool,
    ) -> Result<ProjectorSupport> {
        settings
            .media
            .limits
            .check_decoded_bytes(needs.decoded_bytes)?;
        self.llama.interrupt_signal.store(false, Ordering::Release);
        let path = std::fs::canonicalize(path).context("Projector file is unavailable")?;
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() == 0 || needs.weights_bytes < metadata.len() {
            bail!("Projector weight estimate must cover a non-empty regular file");
        }
        let c_path = CString::new(
            path.to_str()
                .context("Native projector path must be UTF-8")?,
        )?;
        let mut hw = HardwareProber::current();
        let threads = settings
            .optimization
            .resolved_threads(hw.cpu.physical_cores)
            .1;
        let prefer_gpu = prefer_gpu && super::gpu_offload_available();
        let key = ProjectorKey {
            path,
            file_bytes: metadata.len(),
            modified: metadata.modified()?,
            threads,
            prefer_gpu,
            weights_bytes: needs.weights_bytes,
            workspace_bytes: needs.workspace_bytes,
            ram_buffer: settings.optimization.custom_ram_buffer_gb.map(f64::to_bits),
            vram_buffer: settings
                .optimization
                .custom_vram_buffer_gb
                .map(f64::to_bits),
        };
        if self.key.as_ref() == Some(&key) {
            if let Some(projector) = &self.projector {
                let (ram, _) = HardwareProber::probe_live_memory();
                hw.memory.available_gb = bytes_to_gib(ram);
                if cluaiz_plan::media_budget::check_request_ram(
                    needs.workspace_bytes,
                    needs.decoded_bytes,
                    &hw,
                    settings,
                )
                .is_ok()
                {
                    if let Some(p) = &self.progress {
                        p.emit(
                            cluaiz_core::processing::ProcessingStage::ProjectorLoading,
                            cluaiz_core::processing::ProcessingState::Reused,
                            None,
                            None,
                        );
                    }
                    return Ok(projector.support());
                }
                // Only the idle encoder is reclaimed under pressure; the chat plan/model stays intact.
                tracing::info!(
                    free_bytes = ram,
                    "Reclaiming idle projector before media admission"
                );
            }
        }
        self.unload_projector();
        // A zero/unknown live query must not reuse cached free memory for a new allocation.
        let (ram, vram) = HardwareProber::probe_live_memory();
        hw.memory.available_bytes = ram;
        hw.memory.available_gb = bytes_to_gib(ram);
        if let Some(gpu) = hw.gpus.first_mut() {
            gpu.vram_free_bytes = vram;
            gpu.vram_available_gb = bytes_to_gib(vram);
        }
        let budget = MediaBudget::plan(needs, &hw, settings, prefer_gpu)?;
        if let Some(p) = &self.progress {
            p.emit(
                cluaiz_core::processing::ProcessingStage::ProjectorLoading,
                cluaiz_core::processing::ProcessingState::Running,
                None,
                None,
            );
        }
        let projector = Projector::load(&self.llama, &c_path, budget.placement(), threads)?;
        if let Some(p) = &self.progress {
            p.emit(
                cluaiz_core::processing::ProcessingStage::ProjectorLoading,
                cluaiz_core::processing::ProcessingState::Completed,
                None,
                None,
            );
        }
        let after = std::fs::metadata(&key.path)?;
        if after.len() != key.file_bytes || after.modified()? != key.modified {
            bail!("Projector file changed during loading; retry with fresh estimates");
        }
        let support = projector.support();
        self.projector = Some(projector);
        self.key = Some(key);
        self.budget = Some(budget);
        Ok(support)
    }

    /// Clear projector identity and cost along with the native owner; keeps the model loaded.
    pub fn unload_projector(&mut self) {
        self.projector = None;
        self.key = None;
        self.budget = None;
    }

    /// Queries captured from the loaded encoder; does not imply a wired API route.
    pub fn projector_support(&self) -> Option<ProjectorSupport> {
        self.projector.as_ref().map(Projector::support)
    }

    /// Estimate used for the current projector, absent after unload/failure.
    pub fn media_budget(&self) -> Option<MediaBudget> {
        self.budget
    }

    /// May be retained by a cancellation controller without sharing native encoder state.
    pub fn interrupt_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.llama.interrupt_signal)
    }

    /// Generate from actual prepared media using native template/encoder and the existing
    /// sampler loop. The KV prefix of the previous media turn (text and encoded media matched by
    /// content id, as llama-server does) is reused; only what changed is encoded and processed.
    pub fn stream_prepared(
        &mut self,
        request: ChatEnvelope<Vec<PreparedPart>>,
        max_tokens: usize,
        dna: &StructuralDNA,
        settings: &ActiveUserSettings,
        mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        if self.progress.as_ref().is_some_and(|p| p.cancelled()) {
            bail!("Media request cancelled before execution");
        }
        let limits = settings.media.limits;
        let workspace = self
            .key
            .as_ref()
            .context("Media projector is not loaded")?
            .workspace_bytes;
        let additional = MediaInput::additional_bytes(
            &request,
            self.projector_support()
                .context("Media projector is not loaded")?,
            limits,
        )?;
        let mut live = HardwareProber::current();
        let (ram, vram) = HardwareProber::probe_live_memory();
        live.memory.available_gb = bytes_to_gib(ram);
        cluaiz_plan::media_budget::check_request_ram(workspace, additional, &live, settings)?;
        if self
            .budget
            .context("Media budget is unavailable")?
            .placement()
            == cluaiz_plan::media_budget::ProjectorPlacement::Gpu
        {
            let buffer = self
                .budget
                .context("Media budget is unavailable")?
                .device_buffer_bytes();
            if workspace
                .checked_add(buffer)
                .is_none_or(|needed| needed > vram)
            {
                bail!("Insufficient live VRAM for media request workspace");
            }
        }
        self.llama.interrupt_signal.store(false, Ordering::Release);
        let projector = self
            .projector
            .as_mut()
            .context("Media projector is not loaded")?;
        let (text, mut media) = MediaInput::prepare(request, projector.support(), limits)?;
        media.progress = self.progress.clone();
        media.cached = std::mem::take(&mut self.media_cache);
        let prompt = serde_json::to_string(&text)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let consumer_stop = Arc::clone(&stopped);
        let result = super::stream::stream_tokens_with_media(
            &mut self.llama,
            &prompt,
            max_tokens,
            dna,
            &[],
            Box::new(move |piece| {
                let keep_going = callback(piece);
                if !keep_going {
                    consumer_stop.store(true, Ordering::Release);
                }
                keep_going
            }),
            super::stream::StreamOptions {
                media: Some((&mut media, projector)),
                skip_reasoning: self.skip_reasoning.as_deref(),
                cancel: self.cancel.as_deref(),
            },
        );
        let too_large = result
            .as_ref()
            .err()
            .is_some_and(|e| e.is::<super::core::ContextExceeded>());
        if too_large {
            // Refused before the KV was touched: the previous turn stays cached for the next request
            self.media_cache = std::mem::take(&mut media.cached);
        } else if result.is_err()
            || stopped.load(Ordering::Acquire)
            || self.llama.interrupt_signal.load(Ordering::Acquire)
        {
            // SAFETY: exclusive live model/context; an incomplete media turn is never reused.
            unsafe {
                crate::ffi::llama_cpp::llama_memory_clear(
                    crate::ffi::llama_cpp::llama_get_memory(self.llama.ctx_ptr),
                    true,
                );
            }
        }
        let history = result?;
        if stopped.load(Ordering::Acquire) || self.llama.interrupt_signal.load(Ordering::Acquire) {
            bail!("Media generation cancelled/stopped by consumer");
        }
        // The KV now holds the prompt layout plus the answer tokens that were decoded
        if let Some(report) = &self.llama.last_context {
            let decoded = report.n_tokens.saturating_sub(report.prompt_tokens);
            let start = media.prompt_text_tokens.min(history.len());
            let end = (start + decoded).min(history.len());
            self.media_cache = media.cached_after(&history[start..end]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::llama_cpp;
    use cluaiz_models::{capabilities::PackageCapabilities, GgufProber, StructuralDNA};

    struct TestSession {
        session: NativeSession,
        dna: StructuralDNA,
        model: PathBuf,
        projector: PathBuf,
        settings: ActiveUserSettings,
    }

    fn load_test_session(context: u32) -> Result<TestSession> {
        let model = PathBuf::from(std::env::var("CLUAIZ_MEDIA_MODEL")?);
        let projector = PathBuf::from(std::env::var("CLUAIZ_MEDIA_PROJECTOR")?);
        let mut settings = ActiveUserSettings::default();
        settings.optimization.n_threads = Some(2);
        settings.optimization.n_threads_batch = Some(2);
        settings.optimization.custom_context_window = Some(context);
        let mut hw = HardwareProber::with_live_memory();
        hw.gpus.clear(); // test matches the CPU-only native load below
        let geometry = GgufProber::probe_geometry(&model)?;
        let plan = cluaiz_plan::Planner::create_plan(
            model.to_str().context("Test path UTF-8")?,
            &geometry,
            &hw,
            &settings,
        );
        if !plan.is_feasible() {
            bail!("Test model is infeasible: {}", plan.explanation);
        }
        // SAFETY: default parameter factories do not require native handles.
        let (mut mp, mut cp) = unsafe {
            (
                llama_cpp::llama_model_default_params(),
                llama_cpp::llama_context_default_params(),
            )
        };
        mp.n_gpu_layers = 0;
        mp.set_mmap(true);
        mp.use_extra_bufts = false;
        cp.n_ctx = context;
        cp.n_batch = 128;
        cp.n_ubatch = 128;
        cp.n_threads = 2;
        cp.n_threads_batch = 2;
        cp.offload_kqv = 0;
        cp.op_offload = 0;
        let mut dna = StructuralDNA::default();
        let llama = NativeLlama::load(
            model.to_str().context("Test path UTF-8")?,
            mp,
            cp,
            &mut dna,
            0,
            0,
            0,
            None,
        )?;
        Ok(TestSession {
            session: NativeSession::new(llama)?,
            dna,
            model,
            projector,
            settings,
        })
    }

    #[test]
    #[ignore = "requires externally supplied CLUAIZ_MEDIA_MODEL and CLUAIZ_MEDIA_PROJECTOR files"]
    fn test_session_real_projector_query_reuse_reload_unload_and_drop() -> Result<()> {
        let TestSession {
            mut session,
            dna: _,
            model,
            projector,
            mut settings,
        } = load_test_session(512)?;
        assert!(session.projector_support().is_none());
        let needs = MediaRequirements {
            weights_bytes: std::fs::metadata(&projector)?.len(),
            workspace_bytes: settings.media.limits.max_decoded_bytes(),
            decoded_bytes: 2,
        };
        assert!(session
            .ensure_projector(
                &projector,
                MediaRequirements {
                    workspace_bytes: 0,
                    ..needs
                },
                &settings,
                false
            )
            .is_err());
        assert!(session.projector_support().is_none());
        let support = session.ensure_projector(&projector, needs, &settings, false)?;
        assert!(support.vision || support.audio_sample_rate.is_some());
        let declared = PackageCapabilities::inspect(&model)?.supported_inputs();
        if declared.contains(&cluaiz_core::contracts::Modality::Image) {
            assert!(support.vision);
        }
        if declared.contains(&cluaiz_core::contracts::Modality::Audio) {
            assert!(support.audio_sample_rate.is_some());
        }
        let address = session
            .projector
            .as_ref()
            .expect("loaded projector")
            .test_address();
        assert_eq!(
            session.ensure_projector(&projector, needs, &settings, false)?,
            support
        );
        assert_eq!(
            session.ensure_projector(
                &projector,
                MediaRequirements {
                    decoded_bytes: 1,
                    ..needs
                },
                &settings,
                false
            )?,
            support
        );
        assert_eq!(
            session
                .projector
                .as_ref()
                .expect("reused projector")
                .test_address(),
            address
        );
        assert_eq!(
            session.media_budget().expect("budget").placement(),
            cluaiz_plan::media_budget::ProjectorPlacement::Cpu
        );
        let original_limits = settings.media.limits;
        settings.media.limits = cluaiz_core::media_limits::MediaLimits::new(1, 1)?;
        assert!(session
            .ensure_projector(&projector, needs, &settings, false)
            .is_err());
        assert_eq!(
            session
                .projector
                .as_ref()
                .expect("still loaded")
                .test_address(),
            address
        );
        settings.media.limits = original_limits;
        settings.optimization.n_threads_batch = Some(1);
        assert_eq!(
            session.ensure_projector(&projector, needs, &settings, false)?,
            support
        );
        assert_eq!(session.key.as_ref().expect("reloaded identity").threads, 1);
        session.unload_projector();
        assert!(session.projector_support().is_none());
        assert!(session.media_budget().is_none());
        assert!(session.key.is_none());
        assert!(session
            .ensure_projector(
                &projector.with_extension("missing-test-asset"),
                needs,
                &settings,
                false
            )
            .is_err());
        assert!(session.projector_support().is_none());
        let support = session.ensure_projector(&projector, needs, &settings, false)?;
        eprintln!("Native projector: vision={}, audio_rate={:?}, CPU session load/reuse/reload/unload/drop",
            support.vision, support.audio_sample_rate.map(|rate| rate.get()));
        drop(session); // paired owner drops a live projector before its model
        Ok(())
    }

    fn collect(output: &Arc<std::sync::Mutex<String>>) -> Box<dyn FnMut(String) -> bool + Send> {
        let output = Arc::clone(output);
        Box::new(move |piece| {
            output.lock().expect("test output lock").push_str(&piece);
            true
        })
    }

    #[test]
    #[ignore = "requires externally supplied CLUAIZ_MEDIA_MODEL and CLUAIZ_MEDIA_PROJECTOR files"]
    fn test_session_encoded_images_change_answers_and_recover_text() -> Result<()> {
        use base64::Engine;
        use cluaiz_core::{chat::ChatMessage, media::InputPart};
        use image::{DynamicImage, ImageOutputFormat, Rgb, RgbImage};
        use std::io::Cursor;

        let TestSession {
            mut session,
            dna,
            model: _,
            projector,
            settings,
        } = load_test_session(4096)?;
        let support = session.ensure_projector(
            &projector,
            MediaRequirements {
                weights_bytes: std::fs::metadata(&projector)?.len(),
                workspace_bytes: cluaiz_core::GIB,
                decoded_bytes: settings.media.limits.max_decoded_bytes(),
            },
            &settings,
            false,
        )?;
        assert!(support.vision);
        let directory = tempfile::tempdir()?;
        let resolver = cluaiz_media::source::SourceResolver::new(
            directory.path().to_path_buf(),
            settings.media,
            cluaiz_media::formats::FormatRegistry::default(),
        )?;
        let runtime = tokio::runtime::Runtime::new()?;
        let mut history = Vec::new();
        let mut previous_prompt = 0_usize;
        // Test-only encoded fixtures; neither their color nor ground truth enters the prompt.
        for (pixels, format, mime, expected) in [
            ([255, 0, 0], ImageOutputFormat::Png, "image/png", "red"),
            ([0, 0, 255], ImageOutputFormat::Png, "image/png", "blue"),
            (
                [0, 0, 255],
                ImageOutputFormat::Jpeg(90),
                "image/jpeg",
                "blue",
            ),
        ] {
            let mut encoded = Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(RgbImage::from_pixel(128, 128, Rgb(pixels)))
                .write_to(&mut encoded, format)?;
            let request = ChatEnvelope {
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: vec![
                        InputPart::Text {
                            text: "What is the main color of this image? Reply with one color word only."
                                .into(),
                        },
                        InputPart::Image {
                            source: format!(
                                "data:{mime};base64,{}",
                                base64::engine::general_purpose::STANDARD.encode(encoded.get_ref())
                            ),
                        },
                    ],
                }],
                samplers: cluaiz_core::SamplerDefaults {
                    temp: Some(0.0),
                    seed: Some(31),
                    ..Default::default()
                },
                think_mode: "off".into(),
                reasoning_effort: Default::default(),
                reasoning_budget_message: None,
                chat_template_kwargs: None,
                grammar: None,
                response_format: None,
            };
            let mut prepared = runtime.block_on(cluaiz_media::request::prepare_chat(
                request,
                &resolver,
                |_| bail!("Inline image fixtures must not read local files"),
            ))?;
            let current_turn = prepared.messages.clone();
            prepared.messages.splice(0..0, history.clone());
            let output = Arc::new(std::sync::Mutex::new(String::new()));
            session.stream_prepared(prepared, 32, &dna, &settings, collect(&output))?;
            let report = session
                .llama
                .last_context
                .clone()
                .context("Media context report missing")?;
            eprintln!(
                "REAL MEDIA CACHE: cached={} prompt={} dropped={}",
                report.cache_n, report.prompt_tokens, report.dropped_messages
            );
            if !history.is_empty() {
                // Earlier turns, their images included, come from the KV cache instead of being encoded again
                assert_eq!(report.dropped_messages, 0);
                assert!(
                    report.cache_n * 10 >= previous_prompt * 9,
                    "earlier media turn was not reused"
                );
            }
            let answer = output.lock().expect("image output").clone();
            eprintln!("Shared {mime} -> native image encoder -> model answer: {answer}");
            let words: Vec<_> = answer
                .split(|character: char| !character.is_ascii_alphabetic())
                .filter(|word| !word.is_empty())
                .map(str::to_ascii_lowercase)
                .collect();
            assert_eq!(words, [expected], "Image answer must follow actual pixels");
            history.extend(current_turn);
            history.push(ChatMessage {
                role: "assistant".into(),
                content: vec![cluaiz_core::media::PreparedPart::Text { text: answer }],
            });
            previous_prompt = report.prompt_tokens;
        }
        // A text-only follow-up in the same chat reuses every image instead of encoding them again
        let mut follow_up = ChatEnvelope {
            messages: history.clone(),
            samplers: cluaiz_core::SamplerDefaults {
                temp: Some(0.0),
                seed: Some(31),
                ..Default::default()
            },
            think_mode: "off".into(),
            reasoning_effort: Default::default(),
            reasoning_budget_message: None,
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
        };
        follow_up.messages.push(ChatMessage {
            role: "user".into(),
            content: vec![cluaiz_core::media::PreparedPart::Text {
                text: "What color was the first image? Reply with one color word only.".into(),
            }],
        });
        let output = Arc::new(std::sync::Mutex::new(String::new()));
        session.stream_prepared(follow_up, 32, &dna, &settings, collect(&output))?;
        let report = session
            .llama
            .last_context
            .clone()
            .context("Media context report missing")?;
        eprintln!(
            "REAL MEDIA FOLLOW-UP: cached={} prompt={}",
            report.cache_n, report.prompt_tokens
        );
        assert!(
            report.cache_n * 10 >= previous_prompt * 9,
            "follow-up re-encoded earlier images"
        );
        assert!(output
            .lock()
            .expect("follow-up output")
            .to_ascii_lowercase()
            .contains("red"));
        assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
        let recovery = Arc::new(std::sync::Mutex::new(String::new()));
        super::super::stream::stream_tokens(
            &mut session.llama,
            "What is 2 plus 3? Reply with the digit only.",
            32,
            &dna,
            &[],
            collect(&recovery),
        )?;
        assert!(recovery.lock().expect("text recovery").contains('5'));
        Ok(())
    }

    #[test]
    #[ignore = "requires model/projector and CLUAIZ_MEDIA_AUDIO_URL / CLUAIZ_MEDIA_EXPECTED"]
    fn test_session_media_online_audio_and_text_regression() -> Result<()> {
        let url = std::env::var("CLUAIZ_MEDIA_AUDIO_URL")?;
        let expected = std::env::var("CLUAIZ_MEDIA_EXPECTED")?.to_lowercase();
        if expected.trim().is_empty() {
            bail!("Expected transcript fragment is required");
        }
        let TestSession {
            mut session,
            dna,
            model: _,
            projector,
            settings,
        } = load_test_session(4096)?;
        let plain = ChatEnvelope {
            messages: vec![cluaiz_core::chat::ChatMessage::new(
                "user",
                "What is 2 plus 3? Reply with the digit only.",
            )],
            samplers: cluaiz_core::SamplerDefaults {
                temp: Some(0.0),
                seed: Some(31),
                ..Default::default()
            },
            think_mode: "off".into(),
            reasoning_effort: Default::default(),
            reasoning_budget_message: None,
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
        };
        let plain_prompt = serde_json::to_string(&plain)?;
        let text_output = Arc::new(std::sync::Mutex::new(String::new()));
        super::super::stream::stream_tokens(
            &mut session.llama,
            &plain_prompt,
            32,
            &dna,
            &[],
            collect(&text_output),
        )?;
        assert!(text_output.lock().expect("text output").contains('5'));
        let needs = MediaRequirements {
            weights_bytes: std::fs::metadata(&projector)?.len(),
            workspace_bytes: cluaiz_core::GIB,
            decoded_bytes: settings.media.limits.max_decoded_bytes(),
        };
        let support = session.ensure_projector(&projector, needs, &settings, false)?;
        assert!(support.audio_sample_rate.is_some());
        let runtime = tokio::runtime::Runtime::new()?;
        let resolver = cluaiz_media::source::SourceResolver::new(
            std::env::temp_dir().join("cluaiz-media-demo"),
            settings.media,
            cluaiz_media::formats::FormatRegistry::default(),
        )?;
        let request = ChatEnvelope { messages: vec![cluaiz_core::chat::ChatMessage {
            role: "user".into(), content: vec![cluaiz_core::media::InputPart::Text {
                text: "Transcribe the spoken words in this audio. Reply only with the transcript.".into() },
                cluaiz_core::media::InputPart::Audio { source: url }] }], samplers: plain.samplers,
            think_mode: plain.think_mode, reasoning_effort: plain.reasoning_effort,
            reasoning_budget_message: None, chat_template_kwargs: None, grammar: None, response_format: None };
        let prepared = runtime.block_on(cluaiz_media::request::prepare_chat(
            request,
            &resolver,
            |_| bail!("This demo requires an HTTP audio URL, not a local file"),
        ))?;
        let output = Arc::new(std::sync::Mutex::new(String::new()));
        session.stream_prepared(prepared.clone(), 96, &dna, &settings, collect(&output))?;
        let answer = output.lock().expect("audio output").clone();
        eprintln!("Shared HTTP audio -> native encoder -> model answer: {answer}");
        assert!(
            answer.to_lowercase().contains(&expected),
            "Audio response did not match expected fragment: {answer}"
        );
        assert!(session
            .stream_prepared(
                prepared.clone(),
                session.llama.n_ctx as usize + 1,
                &dna,
                &settings,
                Box::new(|_| true)
            )
            .is_err());
        assert!(session
            .stream_prepared(prepared, 96, &dna, &settings, Box::new(|_| false))
            .is_err());
        let recovery = Arc::new(std::sync::Mutex::new(String::new()));
        super::super::stream::stream_tokens(
            &mut session.llama,
            &plain_prompt,
            32,
            &dna,
            &[],
            collect(&recovery),
        )?;
        assert!(recovery.lock().expect("recovery output").contains('5'));
        Ok(())
    }
}
