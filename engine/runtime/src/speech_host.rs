//! Text-to-speech with a GGUF speech model (llama.cpp audio generation, e.g. Qwen3-TTS): its own
//! backbone and audio projector, beside the chat model and the ONNX audio host, so speaking never
//! evicts or touches the chat. One speech model stays loaded; `keep_alive: 0` unloads it after the
//! request, and another speech model replaces it.

use crate::onnx_host::{AudioEvent, AudioOutput};
use anyhow::{Context, Result};
use cluaiz_models::types::manifest::ModelRegistryEntry;
use llama_engine::native::speech::SpeechModel;
use cluaiz_core::ActiveUserSettings;
use crate::speech_plan::select_placement;
pub use llama_engine::native::speech::SpeechOptions;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// Read only the backbone header: listing controls must never load model weights or a GPU session.
pub fn capabilities(entry: &ModelRegistryEntry) -> Result<onnx_engine::audio::tts::capabilities::TtsCapabilities> {
    let (model, projector) = speech_files(entry)?;
    let header = cluaiz_models::GgufProber::read_header(&model)?;
    let package = cluaiz_models::capabilities::PackageCapabilities::inspect(&model)?;
    let reference = package.projector.iter().chain(&package.projector_candidates)
        .any(|p| p.path == projector && p.profile.audio_encoder == Some(true));
    Ok(onnx_engine::audio::tts::capabilities::TtsCapabilities {
        languages: llama_engine::native::speech::language_codes(&header.speech_languages)?,
        language_control: if header.speech_languages.is_empty() { "auto" } else { "explicit" },
        voice_mode: if reference { "reference_source" } else { "generated" }, coherent_input: true,
    })
}

#[derive(Clone, Default)]
pub struct SpeechHost {
    loaded: Arc<Mutex<Option<Resident<SpeechModel>>>>,
    admission: crate::media_admission::MediaAdmission,
    transition: Arc<tokio::sync::Mutex<()>>,
    room: crate::media_room::MediaRoom,
}

struct Resident<T> {
    id: String,
    settings_key: String,
    model: T,
    /// Identity changes even when the next request uses the same model/retention value.
    lease: Arc<()>,
}

impl SpeechHost {
    pub(crate) fn with_transition(transition: Arc<tokio::sync::Mutex<()>>, room: crate::media_room::MediaRoom) -> Self {
        Self { transition, room, ..Default::default() }
    }

    /// [`Self::unload_idle`] for another owner, without keeping this one alive
    pub(crate) fn idle_releaser(&self) -> impl Fn() -> bool + Send + Sync + 'static {
        let loaded = Arc::downgrade(&self.loaded);
        move || loaded.upgrade().is_some_and(|loaded| free_idle(&loaded))
    }

    /// Called under speech admission when another TTS backend replaces this resident.
    pub fn unload(&self) -> Result<()> {
        *self.loaded.lock().map_err(|_| anyhow::anyhow!("Speech host lock poisoned"))? = None;
        Ok(())
    }

    /// Frees the speech model when no request is using it; true when one was freed
    pub fn unload_idle(&self) -> bool {
        free_idle(&self.loaded)
    }

    /// Admit before decoding reference audio; the lease stays owned by the native worker.
    pub fn try_acquire(&self) -> Result<crate::media_admission::MediaLease, tokio::sync::TryAcquireError> {
        self.admission.try_acquire()
    }

    /// Emits distinct WAV parts during generation, then the whole utterance's WAV data URI.
    pub fn dispatch(
        &self,
        entry: ModelRegistryEntry,
        text: String,
        options: SpeechOptions,
        settings: ActiveUserSettings,
        keep_alive: Option<i32>,
        admission: crate::media_admission::MediaLease,
    ) -> mpsc::Receiver<Result<AudioEvent, String>> {
        let (tx, rx) = mpsc::channel(2);
        let loaded = Arc::clone(&self.loaded);
        let transition = Arc::clone(&self.transition);
        let room = self.room.clone();
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let lease = Arc::new(());
            let result = speak(&loaded, &transition, &room, &entry, &text, options, &settings, keep_alive, Arc::clone(&lease), &tx)
                .map(|uri| AudioEvent::Completed(AudioOutput::WavDataUri(uri)))
                .map_err(|e| format!("{e:#}"));
            if result.is_ok() {
                if let Some(minutes) = keep_alive.filter(|v| *v > 0) {
                    let weak = Arc::downgrade(&loaded);
                    handle.spawn(async move {
                        tokio::time::sleep(Duration::from_secs(minutes as u64 * 60)).await;
                        if let Some(loaded) = weak.upgrade() {
                            let _ = tokio::task::spawn_blocking(move || unload_if_current(&loaded, &lease)).await;
                        }
                    });
                }
            }
            let _ = tx.blocking_send(result);
        });
        rx
    }

    /// The loaded speech model's id, if any.
    pub fn resident_model(&self) -> Option<String> {
        self.loaded.lock().ok()?.as_ref().map(|r| r.id.clone())
    }
}

fn free_idle(loaded: &Mutex<Option<Resident<SpeechModel>>>) -> bool {
    loaded.try_lock().ok().and_then(|mut loaded| loaded.take()).is_some()
}

fn speak(
    loaded: &Mutex<Option<Resident<SpeechModel>>>,
    transition: &tokio::sync::Mutex<()>,
    room: &crate::media_room::MediaRoom,
    entry: &ModelRegistryEntry,
    text: &str,
    mut options: SpeechOptions,
    settings: &ActiveUserSettings,
    keep_alive: Option<i32>,
    lease: Arc<()>,
    tx: &mpsc::Sender<Result<AudioEvent, String>>,
) -> Result<String> {
    if tx.is_closed() { anyhow::bail!("Speech request cancelled"); }
    let mut slot = loaded.lock().map_err(|_| anyhow::anyhow!("Speech host lock poisoned"))?;
    if tx.is_closed() { anyhow::bail!("Speech request cancelled"); }
    let (model, projector) = speech_files(entry)?;
    let settings_key = load_key(&model, &projector, settings)?;
    // A fresh load on the GPU is measured after its first speech: (free VRAM before, estimate)
    let mut measuring: Option<(f64, u64)> = None;
    let measure_key = format!("speech:{}", entry.id);
    if slot.as_ref().map(|r| r.id != entry.id || r.settings_key != settings_key).unwrap_or(true) {
        // Free the previous model before loading the next one
        *slot = None;
        // Share the chat/ONNX load-transition owner so two loads cannot plan the same free bytes.
        let _transition = transition.blocking_lock();
        let geometry = cluaiz_models::GgufProber::probe_geometry(&model)?;
        let codec_bytes = std::fs::metadata(&projector)?.len();
        crate::vram_use::load();
        // What this model took on the GPU beyond its estimate last time is not free for it now
        let extra = cluaiz_plan::margin::learned_key(&measure_key).unwrap_or(0.0).max(0.0);
        let decoded = options.speaker_reference.as_ref().map_or(0, |a| a.samples().len().saturating_mul(std::mem::size_of::<f32>()) as u64);
        let mut free_before = 0.0;
        let plan = room.retry_freeing_others("speech", || {
            let mut live = crate::host::native_load::gguf_hardware();
            free_before = live.gpus.first().map_or(0.0, |g| g.vram_available_gb);
            if let Some(gpu) = live.gpus.first_mut() {
                gpu.vram_available_gb = (gpu.vram_available_gb - extra).max(0.0);
            }
            select_placement(&geometry, codec_bytes, decoded, &live, settings)
        })?;
        let placement = plan.placement;
        let gpu = placement.n_gpu_layers != 0 || placement.projector_gpu;
        if gpu {
            measuring = Some((free_before, plan.device_bytes));
        }
        tracing::info!(model = %entry.id, ?placement, host_estimate_bytes = plan.host_bytes,
            device_estimate_bytes = plan.device_bytes, "Speech load admission (estimates, not a reservation)");
        let speech = match SpeechModel::load_with_placement(&model, &projector, placement) {
            Ok(speech) => speech,
            Err(error) if gpu && settings.optimization.n_gpu_layers.is_none() => {
                tracing::warn!(%error, "Automatic speech GPU load failed; checking CPU admission");
                let mut cpu_settings = settings.clone();
                cpu_settings.optimization.n_gpu_layers = Some(0);
                let cpu = select_placement(&geometry, codec_bytes, decoded,
                    &crate::host::native_load::gguf_hardware(), &cpu_settings)?;
                tracing::info!(model = %entry.id, ?cpu, "Speech CPU retry admission (estimates, not a reservation)");
                SpeechModel::load_with_placement(&model, &projector, cpu.placement)?
            }
            Err(error) => return Err(error),
        };
        *slot = Some(Resident { id: entry.id.clone(), settings_key, model: speech, lease: Arc::clone(&lease) });
    }
    // A load can take more RAM than its estimate and leave too little for the request
    if let Err(error) = room.retry_freeing_others("speech", || check_request_budget(settings, 0)) {
        *slot = None;
        return Err(error);
    }
    let resident = slot.as_mut().context("Speech model is not loaded")?;
    resident.lease = lease;
    options.language = options.language
        .map(|l| l.split(['-', '_']).next().unwrap_or(&l).to_ascii_lowercase())
        .filter(|l| !l.is_empty() && l != "auto");
    let mut samples = Vec::new();
    let rate = resident.model.sample_rate();
    let result = resident.model.speak_pcm(text, &options, |part, sample_rate| {
        let retained = samples.len().checked_add(part.len()).and_then(|n| n.checked_mul(24))
            .and_then(|n| n.checked_add(part.len().checked_mul(16)?)).context("Speech output budget overflow")?;
        options.limits.check_decoded_bytes(u64::try_from(retained)?)?;
        // The weights are mapped and fill RAM once speaking starts, so the room may run out here
        let part_bytes = u64::try_from(part.len().checked_mul(16).context("Speech part budget overflow")?)?;
        room.retry_freeing_others("speech", || check_in_flight_budget(settings, part_bytes))?;
        samples.extend_from_slice(part);
        let uri = cluaiz_media::wav::encode_data_uri(part, sample_rate, options.limits)?;
        tx.blocking_send(Ok(AudioEvent::Chunk(uri)))
            .map_err(|_| anyhow::anyhow!("Speech request cancelled"))
    }, || tx.is_closed()).and_then(|()| {
        let wav_bytes = u64::try_from(samples.len().checked_mul(8).context("Speech WAV budget overflow")?)?;
        room.retry_freeing_others("speech", || check_in_flight_budget(settings, wav_bytes))?;
        cluaiz_media::wav::encode_data_uri(&samples, rate, options.limits)
    });
    // The speech model's GPU use grows while it speaks (Qwen3-TTS 1.7B with 5 layers and the
    // codec on the GPU took 2.44 GB, 0.34 GB over its estimate): learn it for the next load
    if let (Ok(_), Some((before, estimate))) = (&result, measuring) {
        let taken = before - crate::vram_use::free_gb();
        let extra = taken - cluaiz_core::bytes_to_gib(estimate);
        if taken > 0.01 && cluaiz_plan::margin::record_key(&measure_key, extra) {
            tracing::info!("📏 [Speech] {} took {:.2} GB on the GPU, {:+.2} GB over its estimate; learned for its next load", entry.id, taken, extra);
            crate::vram_use::save();
        }
    }
    if result.is_err() || keep_alive == Some(0) {
        *slot = None;
    }
    result
}

fn check_request_budget(settings: &ActiveUserSettings, additional: u64) -> Result<()> {
    cluaiz_plan::media_budget::check_request_ram(settings.media.projector_workspace_bytes.get(), additional,
        &cluaiz_hw::HardwareProber::with_live_memory(), settings)
}

/// After the first part the request's workspace is allocated: only its new buffers are checked
fn check_in_flight_budget(settings: &ActiveUserSettings, additional: u64) -> Result<()> {
    cluaiz_plan::media_budget::check_in_flight_ram(additional, &cluaiz_hw::HardwareProber::with_live_memory(), settings)
}

fn load_key(model: &std::path::Path, projector: &std::path::Path, settings: &ActiveUserSettings) -> Result<String> {
    let stamp = |path: &std::path::Path| -> Result<_> {
        let metadata = std::fs::metadata(path)?;
        Ok((metadata.len(), metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?))
    };
    let captured = llama_engine::native::speech::planning_settings(settings);
    let opt = &captured.optimization;
    Ok(serde_json::to_string(&(model, projector, stamp(model)?, stamp(projector)?, opt.load_fingerprint(),
        opt.custom_ram_buffer_gb, settings.media.projector_workspace_bytes))?)
}

fn unload_if_current<T>(loaded: &Mutex<Option<Resident<T>>>, lease: &Arc<()>) -> Result<bool> {
    let mut slot = loaded.lock().map_err(|_| anyhow::anyhow!("Speech host lock poisoned"))?;
    if !slot.as_ref().is_some_and(|r| Arc::ptr_eq(&r.lease, lease)) { return Ok(false); }
    *slot = None;
    tracing::debug!("Speech retention expired; released native model");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_speech_residency_key_tracks_assets_and_load_settings_only() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let model = dir.path().join("backbone");
        let projector = dir.path().join("codec");
        std::fs::write(&model, b"model")?;
        std::fs::write(&projector, b"projector")?;
        let mut settings = ActiveUserSettings::default();
        let first = load_key(&model, &projector, &settings)?;
        settings.optimization.n_gpu_layers = Some(0);
        assert_ne!(first, load_key(&model, &projector, &settings)?);
        let cpu = load_key(&model, &projector, &settings)?;
        settings.optimization.n_threads = Some(3);
        assert_ne!(cpu, load_key(&model, &projector, &settings)?);
        let threads = load_key(&model, &projector, &settings)?;
        std::fs::write(&projector, b"changed projector")?;
        assert_ne!(threads, load_key(&model, &projector, &settings)?);
        std::fs::remove_file(&projector)?;
        assert!(load_key(&model, &projector, &settings).is_err());
        Ok(())
    }

    #[test]
    fn test_speech_admission_is_bounded_and_released_with_worker_lease() -> Result<()> {
        let host = SpeechHost::default();
        let caller = host.try_acquire()?;
        let worker = caller.clone();
        drop(caller);
        assert!(host.try_acquire().is_err());
        drop(worker);
        assert!(host.try_acquire().is_ok());
        Ok(())
    }

    #[test]
    fn test_speech_assets_cannot_escape_the_selected_package() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let model = dir.path().join("backbone");
        let projector = dir.path().join("projector");
        std::fs::write(&model, b"metadata-only fixture, never loaded")?;
        std::fs::write(&projector, b"metadata-only fixture, never loaded")?;
        let mut entry: ModelRegistryEntry = serde_json::from_value(serde_json::json!({
            "id":"fixture", "category":"tts", "format_type":"gguf", "huggingface_repo":"", "local_dir":dir.path(),
            "files":[{"name":"backbone","size_bytes":1,"is_primary":true}], "supported_tasks":["text-to-speech"], "requires_gpu":false,
            "metadata":{"architecture":"fixture","parameters":"","context_window":"", "capabilities":{
                "primary_file":model,"inputs":[],"projector":{"path":projector,"profile":{"audio_generation":true},"compatibility":"unknown"},
                "projector_candidates":[],"diagnostics":[],"asset_snapshot":[]
            }}
        }))?;
        assert_eq!(speech_files(&entry)?.0, model.canonicalize()?);
        let outside = tempfile::NamedTempFile::new()?;
        entry.files[0].name = outside.path().to_string_lossy().into_owned();
        assert!(speech_files(&entry).unwrap_err().to_string().contains("outside"));
        entry.files[0].name = "backbone".into();
        entry.metadata.capabilities.as_mut().context("Fixture capabilities")?.projector.as_mut().context("Fixture projector")?.path = outside.path().to_owned();
        assert!(speech_files(&entry).unwrap_err().to_string().contains("outside"));
        Ok(())
    }
    #[test]
    fn test_expiry_cannot_unload_a_newer_request_or_replacement() -> Result<()> {
        let old = Arc::new(());
        let current = Arc::new(());
        let loaded = Mutex::new(Some(Resident { id: String::new(), settings_key: String::new(), model: (), lease: Arc::clone(&current) }));
        assert!(!unload_if_current(&loaded, &old)?);
        assert!(unload_if_current(&loaded, &current)?);
        assert!(!unload_if_current(&loaded, &current)?);
        *loaded.lock().map_err(|_| anyhow::anyhow!("Test lock poisoned"))? =
            Some(Resident { id: String::new(), settings_key: String::new(), model: (), lease: Arc::new(()) });
        assert!(!unload_if_current(&loaded, &current)?);
        Ok(())
    }
}

/// The backbone (the registry's primary file) and its audio projector (the package's inspected
/// projector that declares audio generation).
fn speech_files(entry: &ModelRegistryEntry) -> Result<(PathBuf, PathBuf)> {
    let dir = PathBuf::from(&entry.local_dir);
    let model = entry
        .files
        .iter()
        .find(|f| f.is_primary)
        .map(|f| dir.join(&f.name))
        .context("Speech model has no primary file")?;
    let projector = entry
        .metadata
        .capabilities
        .as_ref()
        .and_then(|c| {
            c.projector
                .iter()
                .chain(&c.projector_candidates)
                .find(|p| p.profile.audio_generation == Some(true))
                .map(|p| p.path.clone())
        })
        .context("Speech model has no audio projector (mmproj) in its folder")?;
    let root = dir.canonicalize()?;
    let model = model.canonicalize()?;
    let projector = if projector.is_absolute() { projector } else { dir.join(projector) }.canonicalize()?;
    if !model.starts_with(&root) || !projector.starts_with(&root) {
        anyhow::bail!("Speech asset is outside the selected package");
    }
    Ok((model, projector))
}
