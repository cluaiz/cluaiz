//! Text-to-speech with a GGUF speech model (llama.cpp audio generation, e.g. Qwen3-TTS): its own
//! backbone and audio projector, beside the chat model and the ONNX audio host, so speaking never
//! evicts or touches the chat. One speech model stays loaded; `keep_alive: 0` unloads it after the
//! request, and another speech model replaces it.

use crate::onnx_host::{AudioEvent, AudioOutput};
use anyhow::{Context, Result};
use cluaiz_models::types::manifest::ModelRegistryEntry;
use llama_engine::native::speech::SpeechModel;
pub use llama_engine::native::speech::SpeechOptions;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// GPU memory kept free beside the speech model's own files
const GPU_HEADROOM_BYTES: u64 = 768 * 1024 * 1024;

#[derive(Default)]
pub struct SpeechHost {
    loaded: Arc<Mutex<Option<Resident<SpeechModel>>>>,
}

struct Resident<T> {
    id: String,
    model: T,
    /// Identity changes even when the next request uses the same model/retention value.
    lease: Arc<()>,
}

impl SpeechHost {
    /// Emits distinct WAV parts during generation, then the whole utterance's WAV data URI.
    pub fn dispatch(
        &self,
        entry: ModelRegistryEntry,
        text: String,
        options: SpeechOptions,
        keep_alive: Option<i32>,
    ) -> mpsc::Receiver<Result<AudioEvent, String>> {
        let (tx, rx) = mpsc::channel(2);
        let loaded = Arc::clone(&self.loaded);
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let lease = Arc::new(());
            let result = speak(&loaded, &entry, &text, options, keep_alive, Arc::clone(&lease), &tx)
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

fn speak(
    loaded: &Mutex<Option<Resident<SpeechModel>>>,
    entry: &ModelRegistryEntry,
    text: &str,
    mut options: SpeechOptions,
    keep_alive: Option<i32>,
    lease: Arc<()>,
    tx: &mpsc::Sender<Result<AudioEvent, String>>,
) -> Result<String> {
    if tx.is_closed() { anyhow::bail!("Speech request cancelled"); }
    let mut slot = loaded.lock().map_err(|_| anyhow::anyhow!("Speech host lock poisoned"))?;
    if tx.is_closed() { anyhow::bail!("Speech request cancelled"); }
    if slot.as_ref().map(|r| r.id != entry.id).unwrap_or(true) {
        // Free the previous model before loading the next one
        *slot = None;
        let (model, projector) = speech_files(entry)?;
        let size: u64 = [&model, &projector]
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        let (_ram_free, vram_free) = cluaiz_hw::HardwareProber::probe_live_memory();
        let gpu = vram_free > size.saturating_add(GPU_HEADROOM_BYTES);
        let threads = std::thread::available_parallelism().map_or(4, |n| (n.get() / 2).max(1)) as i32;
        tracing::info!("🗣️ [Speech] Loading {} on {}", entry.id, if gpu { "the GPU" } else { "the CPU" });
        let speech = match SpeechModel::load(&model, &projector, gpu, threads) {
            Ok(speech) => speech,
            Err(error) if gpu => {
                tracing::warn!(%error, "Speech model did not load on the GPU; using the CPU");
                SpeechModel::load(&model, &projector, false, threads)?
            }
            Err(error) => return Err(error),
        };
        *slot = Some(Resident { id: entry.id.clone(), model: speech, lease: Arc::clone(&lease) });
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
        samples.extend_from_slice(part);
        let uri = cluaiz_media::wav::encode_data_uri(part, sample_rate, options.limits)?;
        tx.blocking_send(Ok(AudioEvent::Chunk(uri)))
            .map_err(|_| anyhow::anyhow!("Speech request cancelled"))
    }, || tx.is_closed()).and_then(|()| cluaiz_media::wav::encode_data_uri(&samples, rate, options.limits));
    if result.is_err() || keep_alive == Some(0) {
        *slot = None;
    }
    result
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
    fn test_expiry_cannot_unload_a_newer_request_or_replacement() -> Result<()> {
        let old = Arc::new(());
        let current = Arc::new(());
        let loaded = Mutex::new(Some(Resident { id: String::new(), model: (), lease: Arc::clone(&current) }));
        assert!(!unload_if_current(&loaded, &old)?);
        assert!(unload_if_current(&loaded, &current)?);
        assert!(!unload_if_current(&loaded, &current)?);
        *loaded.lock().map_err(|_| anyhow::anyhow!("Test lock poisoned"))? =
            Some(Resident { id: String::new(), model: (), lease: Arc::new(()) });
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
    Ok((model, projector))
}
