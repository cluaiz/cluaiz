//! Text-to-speech with a GGUF speech model (llama.cpp audio generation, e.g. Qwen3-TTS): its own
//! backbone and audio projector, beside the chat model and the ONNX audio host, so speaking never
//! evicts or touches the chat. One speech model stays loaded; `keep_alive: 0` unloads it after the
//! request, and another speech model replaces it.

use crate::onnx_host::{AudioEvent, AudioOutput};
use anyhow::{Context, Result};
use cluaiz_models::types::manifest::ModelRegistryEntry;
use llama_engine::native::speech::{SpeechModel, SpeechOptions};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// GPU memory kept free beside the speech model's own files
const GPU_HEADROOM_BYTES: u64 = 768 * 1024 * 1024;

#[derive(Default)]
pub struct SpeechHost {
    loaded: Arc<Mutex<Option<(String, SpeechModel)>>>,
}

impl SpeechHost {
    /// Speaks `text` with the GGUF speech model `entry`; the result arrives as one WAV data URI.
    pub fn dispatch(
        &self,
        entry: ModelRegistryEntry,
        text: String,
        language: Option<String>,
        keep_alive: Option<i32>,
    ) -> mpsc::Receiver<Result<AudioEvent, String>> {
        let (tx, rx) = mpsc::channel(2);
        let loaded = Arc::clone(&self.loaded);
        tokio::task::spawn_blocking(move || {
            let result = speak(&loaded, &entry, &text, language, keep_alive)
                .map(|uri| AudioEvent::Completed(AudioOutput::WavDataUri(uri)))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.blocking_send(result);
        });
        rx
    }

    /// The loaded speech model's id, if any.
    pub fn resident_model(&self) -> Option<String> {
        self.loaded.lock().ok()?.as_ref().map(|(id, _)| id.clone())
    }
}

fn speak(
    loaded: &Mutex<Option<(String, SpeechModel)>>,
    entry: &ModelRegistryEntry,
    text: &str,
    language: Option<String>,
    keep_alive: Option<i32>,
) -> Result<String> {
    let mut slot = loaded.lock().map_err(|_| anyhow::anyhow!("Speech host lock poisoned"))?;
    if slot.as_ref().map(|(id, _)| id != &entry.id).unwrap_or(true) {
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
        *slot = Some((entry.id.clone(), speech));
    }
    let (_, speech) = slot.as_mut().context("Speech model is not loaded")?;
    let language = language
        .map(|l| l.split(['-', '_']).next().unwrap_or(&l).to_ascii_lowercase())
        .filter(|l| !l.is_empty() && l != "auto");
    let wav = speech.speak(text, &SpeechOptions { language, ..Default::default() })?;
    if keep_alive == Some(0) {
        *slot = None;
    }
    use base64::Engine;
    Ok(format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(wav)))
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
