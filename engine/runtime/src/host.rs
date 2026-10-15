//! 🏛️ Single Engine Host (`engine/runtime`)
//! Central model host governing active execution plans and streaming sessions.

use cluaiz_core::ActiveUserSettings;
use cluaiz_core::{chat::ChatEnvelope, media::PreparedPart};
use cluaiz_hw::SystemInfo;
use cluaiz_models::types::StructuralDNA;
use cluaiz_models::ModelGeometry;
use cluaiz_plan::{ExecutionPlan, Planner};
use llama_engine::native::session::NativeSession;
use llama_engine::{extract_thinking_tags_native, GenerationTimings, NativeLlama, ThinkingReport};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

enum GenerationRequest {
    Text {
        prompt: String,
        expected_key: Option<String>,
    },
    Media(Box<MediaGeneration>),
}

// Keep the large media/options payload out of every legacy text request's enum slot.
struct MediaGeneration {
    request: ChatEnvelope<Vec<PreparedPart>>,
    expected_key: String,
    settings: ActiveUserSettings,
}

pub enum HostResponse {
    TokenStream(mpsc::Receiver<String>),
    Completed(String),
    Error(String),
}

/// A failed generation ends its token stream with one token in this form (see `stream_error`).
fn error_token(message: impl std::fmt::Display) -> String {
    format!("[ERROR: {}]", message)
}

/// The error message when `token` is the stream's error token, else None.
pub fn stream_error(token: &str) -> Option<&str> {
    token
        .strip_prefix("[ERROR: ")
        .and_then(|t| t.strip_suffix(']'))
}

/// llama.cpp's description of the loaded model, the values llama-bench prints.
#[derive(Debug, Clone)]
pub struct ModelDescription {
    /// e.g. "qwen3 4B Q4_K - Medium" (llama_model_desc)
    pub desc: String,
    pub size_bytes: u64,
    pub n_params: u64,
}

pub struct EngineHost {
    /// Audio sessions have their own lifecycle and never evict the chat owner.
    pub onnx: crate::onnx_host::OnnxHost,
    pub current_plan: Arc<Mutex<Option<ExecutionPlan>>>,
    /// Settings the loaded model was loaded with (user settings + request llama params)
    pub loaded_settings: Arc<Mutex<Option<ActiveUserSettings>>>,
    active_session: Arc<Mutex<Option<NativeSession>>>,
    /// Serialize load/unload transitions, including native allocations before publication.
    model_transition: Mutex<()>,
    pub active_model_path: Arc<Mutex<Option<String>>>,
    pub active_fingerprint: Arc<Mutex<Option<String>>>,
    pub dna: Arc<Mutex<StructuralDNA>>,
    pub last_prefilled_tokens: Arc<Mutex<Vec<i32>>>,
    /// llama.cpp timings of the last finished generation
    pub last_timings: Arc<Mutex<Option<GenerationTimings>>>,
    /// Thinking budget and use of the last finished generation
    pub last_thinking: Arc<Mutex<Option<ThinkingReport>>>,
    /// Thinking tags of the loaded model per chat template (template -> tags); cleared on unload
    thinking_tags_cache: Arc<Mutex<Option<(Option<String>, ThinkingTags)>>>,
    /// Full plan banner of the last load (also written to the log)
    pub plan_report: Arc<Mutex<Option<String>>>,
}

type ThinkingTags = (Option<String>, Option<String>);

impl EngineHost {
    pub fn new() -> Self {
        llama_engine::route_llama_logs();
        unsafe {
            llama_engine::ffi::llama_cpp::llama_backend_init();
        }
        Self {
            onnx: crate::onnx_host::OnnxHost::default(),
            current_plan: Arc::new(Mutex::new(None)),
            loaded_settings: Arc::new(Mutex::new(None)),
            active_session: Arc::new(Mutex::new(None)),
            model_transition: Mutex::new(()),
            active_model_path: Arc::new(Mutex::new(None)),
            active_fingerprint: Arc::new(Mutex::new(None)),
            dna: Arc::new(Mutex::new(StructuralDNA::default())),
            last_prefilled_tokens: Arc::new(Mutex::new(Vec::new())),
            last_timings: Arc::new(Mutex::new(None)),
            last_thinking: Arc::new(Mutex::new(None)),
            thinking_tags_cache: Arc::new(Mutex::new(None)),
            plan_report: Arc::new(Mutex::new(None)),
        }
    }

    /// Dynamically resolves thinking start and end markers directly from llama.cpp's native template engine
    /// Each lookup parses the whole template in llama.cpp, so the result is kept per template for the loaded model.
    pub async fn get_thinking_tags(&self, custom_tmpl: Option<&str>) -> ThinkingTags {
        let llama_guard = self.active_session.lock().await;
        if llama_guard.is_some() {
            // Read once at load for the loaded model's template: reuse instead of re-parsing it per message
            let dna = self.dna.lock().await;
            if custom_tmpl == dna.chat_template.as_deref() {
                let tag = |t: &str| Some(t.to_string()).filter(|t| !t.is_empty());
                return (tag(&dna.think_tag_schema), tag(&dna.think_end_schema));
            }
            drop(dna);
            if let Some((tmpl, tags)) = self.thinking_tags_cache.lock().await.as_ref() {
                if tmpl.as_deref() == custom_tmpl {
                    return tags.clone();
                }
            }
        }
        let tags = match llama_guard.as_ref() {
            Some(session) => session.thinking_tags(custom_tmpl),
            None => extract_thinking_tags_native(std::ptr::null(), custom_tmpl),
        };
        if llama_guard.is_some() {
            *self.thinking_tags_cache.lock().await =
                Some((custom_tmpl.map(str::to_string), tags.clone()));
        }
        tags
    }

    /// Checks if a model at the specified path is currently resident in memory
    pub async fn is_model_loaded(&self, model_path: &str) -> bool {
        let llama_guard = self.active_session.lock().await;
        let path_guard = self.active_model_path.lock().await;
        llama_guard.is_some() && path_guard.as_deref() == Some(model_path)
    }

    /// Load key: model path + load-time settings (ctx setting, GPU layers, KV setting, FA, batch, mmap, buffer).
    /// It never includes live free memory, so a loaded model is not re-planned on the next message.
    pub fn load_key(model_path: &str, settings: &ActiveUserSettings) -> String {
        format!(
            "{}|{}",
            model_path,
            settings.optimization.load_fingerprint()
        )
    }

    /// Checks if a model is resident matching the exact load parameters fingerprint
    pub async fn is_model_loaded_with_fingerprint(&self, fingerprint: &str) -> bool {
        let llama_guard = self.active_session.lock().await;
        let fp_guard = self.active_fingerprint.lock().await;
        llama_guard.is_some() && fp_guard.as_deref() == Some(fingerprint)
    }

    /// Single entry point for "make this model ready": the only place that plans + loads.
    /// `settings` = the user's settings, with a request's llama.cpp params already applied on top.
    /// - Same model and same load-time settings already resident -> returns the plan it was loaded with.
    /// - Otherwise: unload the old model first, then measure free memory, plan, and load.
    ///   (Planning before the unload would count the old model's VRAM as used and shrink the context.)
    pub async fn ensure_loaded(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
        settings: &ActiveUserSettings,
    ) -> anyhow::Result<ExecutionPlan> {
        let _transition = self.model_transition.lock().await;
        let path_str = model_path.to_string_lossy().to_string();
        let key = Self::load_key(&path_str, settings);

        if self.is_model_loaded_with_fingerprint(&key).await {
            if let Some(plan) = self.current_plan.lock().await.clone() {
                return Ok(plan);
            }
        }

        self.unload_model_inner().await;
        let hw = cluaiz_hw::HardwareProber::with_live_memory();
        let geometry = cluaiz_models::GgufProber::probe_geometry(model_path)?;
        let plan = Planner::create_plan(model_id, &geometry, &hw, settings);
        if !plan.is_feasible() {
            // Still report why, so the user sees the numbers behind the refusal
            self.record_plan_report(plan.format_telemetry_banner(&geometry, &hw, settings))
                .await;
            return Err(anyhow::anyhow!("{}", plan.explanation));
        }
        let plan = self.load_model_inner(&path_str, &plan, settings).await?;
        *self.current_plan.lock().await = Some(plan.clone());
        *self.loaded_settings.lock().await = Some(settings.clone());
        Ok(plan)
    }

    /// Plan this model would get right now, without loading it.
    /// Returns the loaded plan when the same model + settings is already resident.
    pub async fn preview_plan(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
        settings: &ActiveUserSettings,
    ) -> anyhow::Result<ExecutionPlan> {
        let path_str = model_path.to_string_lossy().to_string();
        if self
            .is_model_loaded_with_fingerprint(&Self::load_key(&path_str, settings))
            .await
        {
            if let Some(plan) = self.current_plan.lock().await.clone() {
                return Ok(plan);
            }
        }
        let hw = cluaiz_hw::HardwareProber::with_live_memory();
        let geometry = cluaiz_models::GgufProber::probe_geometry(model_path)?;
        Ok(Planner::create_plan(model_id, &geometry, &hw, settings))
    }

    /// Plans and prepares model execution
    pub async fn prepare_model(
        &self,
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let plan = Planner::create_plan(model_id, geometry, hardware, settings);
        let mut guard = self.current_plan.lock().await;
        *guard = Some(plan.clone());
        plan
    }

    /// Loads model into unified process memory using calculated ExecutionPlan.
    /// Returns the plan as loaded: if llama.cpp had to fall back (fewer GPU layers / CPU), the returned
    /// plan carries the real n_gpu_layers and says so, so the UI and API never show a placement that did not happen.
    pub async fn load_model(
        &self,
        model_path: &str,
        plan: &ExecutionPlan,
        settings: &ActiveUserSettings,
    ) -> anyhow::Result<ExecutionPlan> {
        let _transition = self.model_transition.lock().await;
        self.load_model_inner(model_path, plan, settings).await
    }

    async fn load_model_inner(
        &self,
        model_path: &str,
        plan: &ExecutionPlan,
        settings: &ActiveUserSettings,
    ) -> anyhow::Result<ExecutionPlan> {
        let geometry = cluaiz_models::GgufProber::probe_geometry(std::path::Path::new(model_path))?;
        let hw = cluaiz_hw::HardwareProber::with_live_memory();

        // The plan carries the exact llama.cpp values; nothing is re-derived here
        let n_gpu_layers = plan.n_gpu_layers;
        let target_fp = Self::load_key(model_path, settings);

        // Same model with the same load-time settings is already resident: nothing to do
        {
            let llama_guard = self.active_session.lock().await;
            let fp_guard = self.active_fingerprint.lock().await;
            if llama_guard.is_some() && fp_guard.as_deref() == Some(&target_fp) {
                return Ok(plan.clone());
            }
        }

        // Plan banner: kept for the CLI's plan view and written to the log
        self.record_plan_report(plan.format_telemetry_banner(&geometry, &hw, settings))
            .await;

        if !plan.is_feasible() {
            return Err(anyhow::anyhow!("{}", plan.explanation));
        }

        // 🛡️ CRITICAL VRAM HYGIENE: Unload previous model before allocating new one!
        // Prevents two models from colliding simultaneously in physical VRAM.
        self.unload_model_inner().await;

        let n_ctx = plan.context_window as u32;
        let mut dna_guard = self.dna.lock().await;
        let llama = load_with_plan(model_path, plan, settings, &hw, &mut dna_guard)?;
        drop(dna_guard);

        let mut loaded_plan = plan.clone();
        if llama.n_gpu_layers_loaded != plan.n_gpu_layers {
            tracing::warn!(
                "⚠️ [EngineHost] llama.cpp could not load the planned n_gpu_layers {}; loaded with {} instead",
                plan.n_gpu_layers, llama.n_gpu_layers_loaded
            );
            loaded_plan.n_gpu_layers = llama.n_gpu_layers_loaded;
            if llama.n_gpu_layers_loaded == 0 {
                loaded_plan.selected_tier = cluaiz_plan::PlacementTier::CpuOnly;
            }
            loaded_plan.explanation = format!(
                "{} | load fallback: planned n_gpu_layers {} failed in llama.cpp, running with {} (memory numbers above are the plan's)",
                plan.explanation, plan.n_gpu_layers, llama.n_gpu_layers_loaded
            );
        }

        let actual_n_ctx = llama.n_ctx;
        cluaiz_core::set_active_context(Some((model_path.to_string(), actual_n_ctx)));
        if actual_n_ctx != n_ctx {
            tracing::warn!(
                "⚠️ [EngineHost] Planned n_ctx {} but llama.cpp created {}",
                n_ctx,
                actual_n_ctx
            );
        }
        let post_hw = cluaiz_hw::HardwareProber::with_live_memory();
        let post_free_vram = post_hw
            .gpus
            .first()
            .map(|g| g.vram_available_gb)
            .unwrap_or(0.0);
        let pre_free_vram = hw.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
        let vram_delta = (pre_free_vram - post_free_vram).max(0.0);

        // Post-load check: VRAM llama.cpp really took vs what the plan predicted
        if n_gpu_layers != 0 && !hw.gpus.is_empty() {
            tracing::info!(
                "🔍 [EngineHost] VRAM planned {:.2} GB, taken {:.2} GB | n_ctx planned {} created {} | n_gpu_layers {} n_cpu_moe {}",
                plan.vram_budget_gb, vram_delta, n_ctx, actual_n_ctx, n_gpu_layers, plan.n_cpu_moe
            );
            if vram_delta < 0.05 {
                tracing::warn!("⚠️ [EngineHost] GPU layers requested but VRAM did not change: is the CUDA backend compiled in?");
            } else if vram_delta > plan.vram_budget_gb + plan.vram_buffer_gb {
                tracing::warn!(
                    "⚠️ [EngineHost] Took {:.2} GB VRAM, more than plan + buffer ({:.2} GB): Windows may move the rest to shared GPU memory",
                    vram_delta, plan.vram_budget_gb + plan.vram_buffer_gb
                );
            }
        }

        let mut llama_guard = self.active_session.lock().await;
        *llama_guard = Some(NativeSession::new(llama)?);

        let mut path_guard = self.active_model_path.lock().await;
        *path_guard = Some(model_path.to_string());

        let mut fp_guard = self.active_fingerprint.lock().await;
        *fp_guard = Some(target_fp);

        let mut tokens_guard = self.last_prefilled_tokens.lock().await;
        tokens_guard.clear();

        tracing::info!(
            "✅ [EngineHost] Model successfully loaded into unified process space: {}",
            model_path
        );
        Ok(loaded_plan)
    }

    async fn record_plan_report(&self, banner: String) {
        tracing::info!("{}", banner);
        *self.plan_report.lock().await = Some(banner);
    }

    /// Safely unloads the active model from unified process memory
    pub async fn unload_model(&self) {
        let _transition = self.model_transition.lock().await;
        self.unload_model_inner().await;
    }

    async fn unload_model_inner(&self) {
        let mut llama_guard = self.active_session.lock().await;
        *llama_guard = None;
        cluaiz_core::set_active_context(None);
        *self.current_plan.lock().await = None;
        *self.loaded_settings.lock().await = None;
        *self.thinking_tags_cache.lock().await = None;
        let mut path_guard = self.active_model_path.lock().await;
        *path_guard = None;
        let mut fp_guard = self.active_fingerprint.lock().await;
        *fp_guard = None;
        let mut tokens_guard = self.last_prefilled_tokens.lock().await;
        tokens_guard.clear();
        tracing::info!("♻️ [EngineHost] Model unloaded from process memory");
    }

    /// Dispatches prompt generation stream directly through static llama engine
    pub async fn dispatch_stream(&self, prompt: &str, max_tokens: usize) -> HostResponse {
        self.dispatch_request(
            GenerationRequest::Text {
                prompt: prompt.to_owned(),
                expected_key: None,
            },
            max_tokens,
            None,
        )
        .await
    }

    /// API text requests retain their selected model identity and independent cancellation.
    pub async fn dispatch_chat_text(
        &self,
        prompt: &str,
        max_tokens: usize,
        model_path: &std::path::Path,
        settings: &ActiveUserSettings,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> HostResponse {
        self.dispatch_request(
            GenerationRequest::Text {
                prompt: prompt.to_owned(),
                expected_key: Some(Self::load_key(&model_path.to_string_lossy(), settings)),
            },
            max_tokens,
            Some(cancel),
        )
        .await
    }

    /// Typed media remains owned until the serialized worker finishes. The load key
    /// prevents a queued request from being evaluated by a different model/configuration.
    pub async fn dispatch_prepared(
        &self,
        request: ChatEnvelope<Vec<PreparedPart>>,
        max_tokens: usize,
        model_path: &std::path::Path,
        settings: &ActiveUserSettings,
        cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> HostResponse {
        if request
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .all(|part| matches!(part, PreparedPart::Text { .. }))
        {
            // Extracted documents are text inputs; they do not require a vision projector.
            let request = request
                .try_map_messages(|messages| async move {
                    messages
                        .into_iter()
                        .map(|message| {
                            let mut text = String::new();
                            for part in message.content {
                                match part {
                                    PreparedPart::Text { text: value } => text.push_str(&value),
                                    _ => anyhow::bail!("Non-text part in text preparation"),
                                }
                            }
                            Ok(cluaiz_core::ChatMessage {
                                role: message.role,
                                content: text,
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()
                })
                .await;
            let request = match request {
                Ok(request) => request,
                Err(error) => return HostResponse::Error(error.to_string()),
            };
            return self
                .dispatch_request(
                    GenerationRequest::Text {
                        prompt: request.to_prompt(),
                        expected_key: Some(Self::load_key(&model_path.to_string_lossy(), settings)),
                    },
                    max_tokens,
                    cancel,
                )
                .await;
        }
        self.dispatch_request(
            GenerationRequest::Media(Box::new(MediaGeneration {
                request,
                expected_key: Self::load_key(&model_path.to_string_lossy(), settings),
                settings: settings.clone(),
            })),
            max_tokens,
            cancel,
        )
        .await
    }

    async fn dispatch_request(
        &self,
        request: GenerationRequest,
        max_tokens: usize,
        cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> HostResponse {
        let (tx, rx) = mpsc::channel::<String>(128);

        let llama_arc = self.active_session.clone();
        let path_arc = self.active_model_path.clone();
        let fingerprint_arc = self.active_fingerprint.clone();
        let dna_arc = self.dna.clone();
        let tokens_arc = self.last_prefilled_tokens.clone();
        let timings_arc = self.last_timings.clone();
        let thinking_arc = self.last_thinking.clone();

        tokio::task::spawn_blocking(move || {
            let mut llama_guard = llama_arc.blocking_lock();
            if let Some(ref mut llama) = *llama_guard {
                let dna_guard = dna_arc.blocking_lock();
                let mut tokens_guard = tokens_arc.blocking_lock();
                let expected_key = match &request {
                    GenerationRequest::Text { expected_key, .. } => expected_key.as_deref(),
                    GenerationRequest::Media(media) => Some(media.expected_key.as_str()),
                };
                if expected_key
                    .is_some_and(|key| fingerprint_arc.blocking_lock().as_deref() != Some(key))
                    || cancel
                        .as_ref()
                        .is_some_and(|signal| signal.load(std::sync::atomic::Ordering::Acquire))
                {
                    tokens_guard.clear();
                    *timings_arc.blocking_lock() = None;
                    *thinking_arc.blocking_lock() = None;
                    let message = if cancel
                        .as_ref()
                        .is_some_and(|signal| signal.load(std::sync::atomic::Ordering::Acquire))
                    {
                        "Request cancelled before execution"
                    } else {
                        "Model/configuration changed before execution; retry"
                    };
                    let _ = tx.blocking_send(error_token(message));
                    return;
                }

                let tx_clone = tx.clone();
                let callback = Box::new(move |token: String| {
                    if cancel
                        .as_deref()
                        .unwrap_or(&cluaiz_core::GLOBAL_CANCEL_SIGNAL)
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return false;
                    }
                    tx_clone.blocking_send(token).is_ok()
                });

                let result = match request {
                    GenerationRequest::Text { prompt, .. } => {
                        llama.stream_text(&prompt, max_tokens, &dna_guard, &tokens_guard, callback)
                    }
                    GenerationRequest::Media(media) => {
                        let MediaGeneration {
                            request, settings, ..
                        } = *media;
                        tokens_guard.clear();
                        (|| -> anyhow::Result<Vec<i32>> {
                            let model_path = path_arc.blocking_lock().clone().ok_or_else(|| {
                                anyhow::anyhow!("Loaded model identity is unavailable")
                            })?;
                            let package =
                                cluaiz_models::capabilities::PackageCapabilities::inspect(
                                    std::path::Path::new(&model_path),
                                )?;
                            let supported = package.supported_inputs();
                            if request
                                .messages
                                .iter()
                                .flat_map(|m| &m.content)
                                .any(|part| !supported.contains(&part.modality()))
                            {
                                anyhow::bail!("Selected model does not declare support for every request input");
                            }
                            let projector = package.projector.ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Compatible media projector is missing or ambiguous"
                                )
                            })?;
                            let needs = cluaiz_plan::media_budget::MediaRequirements {
                                weights_bytes: std::fs::metadata(&projector.path)?.len(),
                                workspace_bytes: settings.media.projector_workspace_bytes.get(),
                                decoded_bytes: settings.media.limits.max_decoded_bytes(),
                            };
                            llama.ensure_projector(
                                &projector.path,
                                needs,
                                &settings,
                                llama.gpu_layers() != 0,
                            )?;
                            llama.stream_prepared(
                                request, max_tokens, &dna_guard, &settings, callback,
                            )?;
                            Ok(Vec::new()) // media embeddings never become text-only prefix history
                        })()
                    }
                };
                match result {
                    Ok(new_tokens) => {
                        *tokens_guard = new_tokens;
                        // Stored before the sender drops, so it is ready when the stream ends
                        *timings_arc.blocking_lock() = Some(llama.timings());
                        *thinking_arc.blocking_lock() = llama.thinking_report();
                    }
                    Err(e) => {
                        tokens_guard.clear();
                        *timings_arc.blocking_lock() = None;
                        *thinking_arc.blocking_lock() = None;
                        let _ = tx.blocking_send(error_token(e));
                    }
                }
            } else {
                let _ = tx.blocking_send(error_token("No model loaded in EngineHost"));
            }
        });

        HostResponse::TokenStream(rx)
    }

    /// Nonblocking input evidence for planning; a busy or different model is unverified.
    pub fn input_support(
        &self,
        package: &cluaiz_models::capabilities::PackageCapabilities,
    ) -> crate::media_support::MediaSupportReport {
        use crate::media_support::{resolve_support, ValidatedPairing, CURRENT_NATIVE_CHAT_INPUTS};
        let (Ok(session), Ok(model)) = (
            self.active_session.try_lock(),
            self.active_model_path.try_lock(),
        ) else {
            return resolve_support(package, CURRENT_NATIVE_CHAT_INPUTS, None, &[]);
        };
        let mut inputs = CURRENT_NATIVE_CHAT_INPUTS.to_vec();
        let selected = model
            .as_deref()
            .and_then(|path| std::path::Path::new(path).canonicalize().ok())
            .is_some_and(|path| path == package.primary_file);
        if let Some(support) = session
            .as_ref()
            .filter(|_| selected)
            .and_then(NativeSession::projector_support)
        {
            inputs.retain(|input| match input {
                cluaiz_core::Modality::Image => support.vision,
                cluaiz_core::Modality::Audio => support.audio_sample_rate.is_some(),
                cluaiz_core::Modality::Text => true,
            });
        }
        let pairing = model
            .as_deref()
            .filter(|_| selected)
            .zip(session.as_ref().and_then(NativeSession::projector_path))
            .map(|(model, projector)| ValidatedPairing {
                model: std::path::Path::new(model),
                projector,
            });
        resolve_support(package, &inputs, pairing, &[])
    }

    /// Availability and attachment descriptors for existing general model endpoints.
    pub async fn media_support(
        &self,
        package: &cluaiz_models::capabilities::PackageCapabilities,
    ) -> crate::media_support::MediaSupportReport {
        use crate::media_support::AttachmentRoute;
        let mut report = self.input_support(package);
        let mut formats = cluaiz_media::formats::attachment_formats();
        formats.extend(cluaiz_media::formats::document_attachment_formats());
        let mut routes: Vec<_> = formats
            .iter()
            .map(|(input, format)| AttachmentRoute {
                input: *input,
                format,
            })
            .collect();
        let video_formats = tokio::task::spawn_blocking(cluaiz_media::video::attachment_formats)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        routes.extend(video_formats.iter().map(|format| AttachmentRoute {
            input: cluaiz_core::Modality::Image,
            format,
        }));
        report.set_attachment_routes(&routes);
        report
    }

    /// llama.cpp timings of the last finished generation (None before the first one)
    pub async fn last_timings(&self) -> Option<GenerationTimings> {
        *self.last_timings.lock().await
    }

    /// Thinking budget and use of the last finished generation (None when the model did not think)
    pub async fn last_thinking(&self) -> Option<ThinkingReport> {
        self.last_thinking.lock().await.clone()
    }

    /// llama.cpp's description of the loaded model (None when nothing is loaded)
    pub async fn model_description(&self) -> Option<ModelDescription> {
        let guard = self.active_session.lock().await;
        guard.as_ref().map(|l| {
            let (desc, size_bytes, n_params) = l.describe();
            ModelDescription {
                desc,
                size_bytes,
                n_params,
            }
        })
    }

    /// llama-bench's measurement on the loaded model: `n_prompt` random tokens decoded in batches,
    /// then `n_gen` tokens one by one, `reps` times after one warmup. Returns nanoseconds per rep.
    /// The KV cache is cleared, so the next chat message prefills from scratch.
    pub async fn bench(
        &self,
        n_prompt: usize,
        n_gen: usize,
        reps: usize,
    ) -> anyhow::Result<Vec<u64>> {
        let llama_arc = self.active_session.clone();
        let tokens_arc = self.last_prefilled_tokens.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = llama_arc.blocking_lock();
            let llama = guard
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("No model loaded in EngineHost"))?;
            tokens_arc.blocking_lock().clear();
            llama.bench(n_prompt, n_gen, reps)
        })
        .await?
    }
}

/// Builds the llama.cpp model/context params from the plan and loads the model.
/// Synchronous on purpose: the params and the MoE override list hold raw C pointers,
/// which must not live inside an async state machine.
fn load_with_plan(
    model_path: &str,
    plan: &ExecutionPlan,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
    dna: &mut StructuralDNA,
) -> anyhow::Result<NativeLlama> {
    let n_gpu_layers = plan.n_gpu_layers;
    let (type_k, type_v) = plan.kv_cache_type.to_ggml_types();
    let (mut model_params, mut ctx_params) = unsafe {
        (
            llama_engine::ffi::llama_cpp::llama_model_default_params(),
            llama_engine::ffi::llama_cpp::llama_context_default_params(),
        )
    };
    model_params.n_gpu_layers = n_gpu_layers;
    // STEP 3: MoE over the RAM limit reads experts from disk -> mmap on, CPU repack off
    model_params.set_mmap(plan.use_mmap(settings));
    if plan.disk_stream_gb > 0.0 {
        model_params.use_extra_bufts = false;
    }
    // MoE split: experts of the first n_cpu_moe blocks in RAM (llama.cpp --n-cpu-moe)
    let moe_overrides = llama_engine::moe_override::CpuMoeOverrides::new(plan.n_cpu_moe as usize);
    model_params.tensor_buft_overrides = moe_overrides.as_ptr();

    ctx_params.n_ctx = plan.context_window;
    ctx_params.n_batch = settings.optimization.resolved_batch_size();
    ctx_params.n_ubatch = settings.optimization.resolved_ubatch_size();
    ctx_params.type_k = type_k;
    ctx_params.type_v = type_v;
    // llama.cpp flash_attn_type: -1 = auto (llama.cpp checks the device), 0 = off, 1 = on
    ctx_params.flash_attn_type = match settings.optimization.flash_attention {
        cluaiz_core::FeatureState::On => 1,
        cluaiz_core::FeatureState::Off => 0,
        cluaiz_core::FeatureState::Auto => -1,
    };
    // KV next to its GPU layers, except CTX rule 5B (weights on GPU, whole KV in RAM)
    ctx_params.offload_kqv = if n_gpu_layers != 0 && plan.kv_offload {
        1
    } else {
        0
    };
    ctx_params.op_offload = if n_gpu_layers != 0 && plan.op_offload {
        1
    } else {
        0
    };
    // Same default as llama.cpp's CLI (physical cores); llama_context_default_params alone gives 4
    let (n_threads, n_threads_batch) = settings
        .optimization
        .resolved_threads(hw.cpu.physical_cores);
    ctx_params.n_threads = n_threads;
    ctx_params.n_threads_batch = n_threads_batch;
    ctx_params.no_perf = 0; // keep llama.cpp's timing counters for the speed report

    // llama.cpp only reads the override list during load; `moe_overrides` lives until this returns
    NativeLlama::load(model_path, model_params, ctx_params, dna, 0, 0, 0, None)
}
