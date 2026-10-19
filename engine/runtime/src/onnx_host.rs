//! Serialized ONNX task lifecycle, typed results and request-specific retention.
use anyhow::{bail, Context, Result};
use cluaiz_core::ActiveUserSettings;
use cluaiz_models::{
    capabilities::PackageCapabilities, prober::OnnxProber, types::manifest::ModelRegistryEntry,
};
pub use onnx_engine::media::{AudioInput, AudioOutput, AudioTask, SttOptions};
pub use onnx_engine::audio::tts::is_experimental_family;
pub use onnx_engine::text::{EmbeddingInput, EmbeddingOutput};
use onnx_engine::OnnxEngine;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct OnnxModel {
    pub id: String,
    dir: PathBuf,
    primary: PathBuf,
    graphs: Vec<PathBuf>,
    asset_key: String,
    entry: ModelRegistryEntry,
}

impl OnnxModel {
    /// The package's TTS voices (empty for a single-voice model).
    pub fn voices(&self) -> Vec<onnx_engine::audio::tts::languages::Voice> {
        onnx_engine::audio::tts::languages::list_voices(&self.dir)
    }

    /// The languages this speech-to-text package can transcribe (empty for a one-language model).
    pub fn stt_languages(&self) -> Vec<String> {
        onnx_engine::audio::stt::families::tokenizer_languages(&self.dir)
    }

    /// Resolve declared assets inside the selected installed package only.
    pub fn from_registry(entry: &ModelRegistryEntry, task: AudioTask) -> Result<Self> {
        if !entry
            .supported_tasks
            .iter()
            .any(|t| AudioTask::parse(t).ok() == Some(task))
        {
            bail!("Selected model does not declare task {}", task.as_str());
        }
        Self::from_package(entry)
    }

    /// Validate an explicit package; graph signatures remain authoritative at load.
    pub fn from_package(entry: &ModelRegistryEntry) -> Result<Self> {
        if !entry.format_type.eq_ignore_ascii_case("onnx") {
            bail!("This ONNX task owner requires an ONNX package");
        }
        let dir = std::fs::canonicalize(&entry.local_dir)?;
        let primary = entry
            .files
            .iter()
            .find(|f| f.is_primary)
            .context("ONNX package requires an explicit primary graph")?;
        let primary = std::fs::canonicalize(dir.join(&primary.name))?;
        let mut graphs = Vec::new();
        let mut stamps = Vec::new();
        for file in &entry.files {
            let path = std::fs::canonicalize(dir.join(&file.name))?;
            if !path.starts_with(&dir) || !path.is_file() {
                bail!("Declared asset is outside the model package or not a file");
            }
            let meta = std::fs::metadata(&path)?;
            stamps.push((
                file.name.clone(),
                meta.len(),
                meta.modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
            ));
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
            {
                graphs.push(path);
            }
        }
        if !graphs.contains(&primary) {
            bail!("Primary asset is not a declared ONNX graph");
        }
        let snapshot = PackageCapabilities::asset_snapshot(&dir);
        let missing = missing_assets(&primary, &snapshot)?;
        if !missing.is_empty() {
            bail!(
                "Required ONNX helper assets are missing: {}",
                missing.join(", ")
            );
        }
        graphs.sort_by_key(|p| (p != &primary, p.clone()));
        stamps.sort_by(|a, b| a.0.cmp(&b.0));
        let asset_key = serde_json::to_string(&(entry, stamps, snapshot))?;
        Ok(Self {
            id: entry.id.clone(),
            dir,
            primary,
            graphs,
            asset_key,
            entry: entry.clone(),
        })
    }
}

/// The package's missing helper assets. Reading every graph's signature takes a noticeable part
/// of a second for large models and every request validates its package, so the answer is kept
/// for as long as the package's files (names, sizes, times) stay the same.
fn missing_assets(
    primary: &std::path::Path,
    snapshot: &[cluaiz_models::capabilities::PackageAssetStamp],
) -> Result<Vec<String>> {
    static KNOWN: Mutex<Vec<(String, Vec<String>)>> = Mutex::new(Vec::new());
    let key = format!("{}|{}", primary.display(), serde_json::to_string(snapshot)?);
    if let Some(missing) = KNOWN
        .lock()
        .ok()
        .and_then(|known| known.iter().find(|(k, _)| *k == key).map(|(_, m)| m.clone()))
    {
        return Ok(missing);
    }
    let missing = PackageCapabilities::inspect(primary)?.missing_onnx_assets();
    if let Ok(mut known) = KNOWN.lock() {
        known.retain(|(k, _)| !k.starts_with(&format!("{}|", primary.display())));
        known.push((key, missing.clone()));
    }
    Ok(missing)
}

/// Speech-to-text runs the encoder, the uncached decoder and (when packaged) the decoder that
/// continues from cached keys/values. A merged decoder (`use_cache_branch`) repeats both and is
/// not loaded, so it takes no GPU memory or load time. Signatures are read without the weights;
/// an unreadable graph is kept.
fn stt_graphs(graphs: &[PathBuf]) -> Vec<PathBuf> {
    graphs
        .iter()
        .filter(|path| {
            OnnxProber::read_signature(path).map_or(true, |signature| {
                !signature.inputs.iter().any(|input| input.name == "use_cache_branch")
            })
        })
        .cloned()
        .collect()
}

#[derive(Debug)]
pub enum AudioEvent {
    Chunk(String),
    Completed(AudioOutput),
}

struct Resident {
    key: String,
    id: String,
    generation: u64,
    engine: OnnxEngine,
    package_key: String,
    readiness: Option<crate::media_support::ValidatedOnnxTask>,
    provider_settings_key: String,
}

#[derive(Default)]
struct AudioState {
    resident: Option<Resident>,
    generation: u64,
}

/// One native audio request executes at a time; mutex guards never cross await.
#[derive(Clone, Default)]
pub struct OnnxHost {
    state: Arc<Mutex<AudioState>>,
}

struct AudioExecution {
    model: OnnxModel,
    task: AudioTask,
    input: AudioInput,
    options: SttOptions,
    settings: ActiveUserSettings,
    keep_alive: Option<i32>,
}

struct OnnxLease<'a> {
    model: &'a OnnxModel,
    task: &'a str,
    settings: &'a ActiveUserSettings,
    keep_alive: Option<i32>,
    handle: &'a tokio::runtime::Handle,
}

impl OnnxHost {
    pub fn dispatch(
        &self,
        model: OnnxModel,
        task: AudioTask,
        input: AudioInput,
        options: SttOptions,
        settings: ActiveUserSettings,
        keep_alive: Option<i32>,
    ) -> mpsc::Receiver<Result<AudioEvent, String>> {
        let (tx, rx) = mpsc::channel(8);
        let owner = self.clone();
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let result = owner.execute(
                AudioExecution {
                    model,
                    task,
                    input,
                    options,
                    settings,
                    keep_alive,
                },
                &tx,
                &handle,
            );
            if let Err(error) = result {
                let _ = tx.blocking_send(Err(format!("{error:#}")));
            }
        });
        rx
    }

    fn execute(
        &self,
        request: AudioExecution,
        tx: &mpsc::Sender<Result<AudioEvent, String>>,
        handle: &tokio::runtime::Handle,
    ) -> Result<()> {
        let AudioExecution {
            model,
            task,
            input,
            options,
            settings,
            keep_alive,
        } = request;
        if tx.is_closed() {
            bail!("Audio output consumer stopped before execution");
        }
        let mut emit = |chunk| tx.blocking_send(Ok(AudioEvent::Chunk(chunk))).is_ok();
        let output = self.with_engine(
            OnnxLease {
                model: &model,
                task: task.as_str(),
                settings: &settings,
                keep_alive,
                handle,
            },
            |engine| {
                let graphs = match task {
                    AudioTask::TextToSpeech => model.graphs.clone(),
                    _ => stt_graphs(&model.graphs),
                };
                engine.load_audio_package(&model.dir, &model.primary, &graphs, task)
            },
            |engine| {
                engine.execute_prepared_audio(
                    task,
                    &input,
                    &options,
                    settings.media.limits,
                    Some(&mut emit),
                )
            },
        )?;
        tx.blocking_send(Ok(AudioEvent::Completed(output)))
            .map_err(|_| anyhow::anyhow!("Audio output consumer stopped"))?;
        Ok(())
    }

    /// Text batches and prepared images reuse the same native lease owner.
    pub async fn embed(
        &self,
        model: OnnxModel,
        inputs: Vec<onnx_engine::text::EmbeddingInput>,
        settings: ActiveUserSettings,
        keep_alive: Option<i32>,
    ) -> Result<Vec<onnx_engine::text::EmbeddingOutput>> {
        let owner = self.clone();
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            owner.with_engine(
                OnnxLease {
                    model: &model,
                    task: "embedding",
                    settings: &settings,
                    keep_alive,
                    handle: &handle,
                },
                |engine| engine.load_embedding_package(&model.dir, &model.primary),
                |engine| {
                    let mut outputs = Vec::new();
                    let mut bytes = 0u64;
                    for input in &inputs {
                        let output = engine.execute_embedding(input, settings.media.limits)?;
                        bytes = (output.vector.len() as u64)
                            .checked_mul(4)
                            .and_then(|size| bytes.checked_add(size))
                            .context("Embedding batch output size overflow")?;
                        settings.media.limits.check_decoded_bytes(bytes)?;
                        outputs.push(output);
                    }
                    Ok(outputs)
                },
            )
        })
        .await?
    }

    fn with_engine<T>(
        &self,
        lease: OnnxLease<'_>,
        load: impl Fn(&mut OnnxEngine) -> Result<()>,
        mut execute: impl FnMut(&OnnxEngine) -> Result<T>,
    ) -> Result<T> {
        let OnnxLease {
            model,
            task,
            settings,
            keep_alive,
            handle,
        } = lease;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("ONNX owner mutex poisoned"))?;
        if OnnxModel::from_package(&model.entry)?.asset_key != model.asset_key {
            bail!("Selected ONNX package changed while queued; retry with current assets");
        }
        let key = serde_json::to_string(&(&model.asset_key, task, &settings.onnx))?;
        state.generation = state
            .generation
            .checked_add(1)
            .context("ONNX request generation overflow")?;
        let generation = state.generation;
        if state.resident.as_ref().is_none_or(|r| r.key != key) {
            state.resident = None; // Free the previous graph before allocating its replacement.
            let mut engine = OnnxEngine::with_settings(settings.onnx.clone())?;
            load(&mut engine)?;
            state.resident = Some(Resident {
                key: key.clone(),
                id: model.id.clone(),
                generation,
                engine,
                package_key: model.asset_key.clone(),
                readiness: None,
                provider_settings_key: serde_json::to_string(&settings.onnx)?,
            });
        }
        let resident = state
            .resident
            .as_mut()
            .context("ONNX session was not published")?;
        resident.generation = generation;
        let mut result = execute(&resident.engine);
        // TTS has not delivered output yet; a retry cannot duplicate partial STT text.
        if task == "text_to_speech"
            && result
                .as_ref()
                .err()
                .is_some_and(|error| resident.engine.permits_cpu_execution_retry(error))
        {
            let provider_error = result
                .as_ref()
                .err()
                .map(|error| format!("{error:#}"))
                .unwrap_or_default();
            tracing::warn!(model = %model.id, error = %provider_error, "TTS GPU execution failed; releasing session before CPU retry");
            state.resident = None;
            let mut cpu_settings = settings.onnx.clone();
            cpu_settings.n_gpu_layers = 0;
            let mut engine = OnnxEngine::with_settings(cpu_settings)?;
            load(&mut engine)
                .with_context(|| format!("TTS CPU retry load failed after: {provider_error}"))?;
            result = execute(&engine)
                .with_context(|| format!("TTS CPU retry failed after: {provider_error}"));
            state.resident = Some(Resident {
                key: key.clone(),
                id: model.id.clone(),
                generation,
                engine,
                package_key: model.asset_key.clone(),
                readiness: None,
                provider_settings_key: serde_json::to_string(&settings.onnx)?,
            });
        }
        let resident = state
            .resident
            .as_mut()
            .context("ONNX session was not published")?;
        if result.is_ok() {
            use cluaiz_core::Modality;
            let readiness = match task {
                "text_to_speech" => Some(("text-to-speech", vec![Modality::Text])),
                "speech_to_text" => Some(("automatic-speech-recognition", vec![Modality::Audio])),
                "embedding" => {
                    let session = resident
                        .engine
                        .acquire_session()
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    let session = session
                        .lock()
                        .map_err(|_| anyhow::anyhow!("ONNX session mutex poisoned"))?;
                    Some(
                        if session.inputs().iter().any(|i| i.name() == "pixel_values") {
                            ("image-feature-extraction", vec![Modality::Image])
                        } else {
                            ("embedding", vec![Modality::Text])
                        },
                    )
                }
                _ => None,
            };
            resident.readiness =
                readiness.map(|(task, inputs)| crate::media_support::ValidatedOnnxTask {
                    task: task.into(),
                    inputs,
                });
        }
        if result.is_err() || keep_alive == Some(0) {
            state.resident = None;
        }
        drop(state);
        if let Some(minutes) = keep_alive.filter(|v| *v > 0) {
            let owner = self.clone();
            handle.spawn(async move {
                tokio::time::sleep(Duration::from_secs(minutes as u64 * 60)).await;
                let _ =
                    tokio::task::spawn_blocking(move || owner.unload_if_current(&key, generation))
                        .await;
            });
        }
        result
    }

    fn unload_if_current(&self, key: &str, generation: u64) -> Result<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("ONNX owner mutex poisoned"))?;
        if !state
            .resident
            .as_ref()
            .is_some_and(|r| r.key == key && r.generation == generation)
        {
            return Ok(false);
        }
        state.resident = None;
        Ok(true)
    }

    /// Inspect resident audio identity without exposing mutable native sessions.
    pub async fn resident_model(&self) -> Result<Option<String>> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            let state = owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("ONNX owner mutex poisoned"))?;
            Ok(state.resident.as_ref().map(|r| r.id.clone()))
        })
        .await?
    }

    /// Trusted successful task evidence must match the current package/config identity.
    pub async fn readiness(
        &self,
        entry: &ModelRegistryEntry,
    ) -> Result<Option<crate::media_support::ValidatedOnnxTask>> {
        let model = OnnxModel::from_package(entry)?;
        let provider_settings_key = serde_json::to_string(&cluaiz_core::Settings::current().onnx)?;
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            let state = owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("ONNX owner mutex poisoned"))?;
            Ok(state
                .resident
                .as_ref()
                .filter(|r| {
                    r.package_key == model.asset_key
                        && r.provider_settings_key == provider_settings_key
                })
                .and_then(|r| r.readiness.clone()))
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expiry_rejects_old_request_and_different_model() -> Result<()> {
        let host = OnnxHost::default();
        host.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Test mutex poisoned"))?
            .resident = Some(Resident {
            key: "current".into(),
            id: "selected".into(),
            generation: 2,
            engine: OnnxEngine::with_settings(Default::default())?,
            package_key: "fixture".into(),
            readiness: None,
            provider_settings_key: "fixture".into(),
        });
        assert!(!host.unload_if_current("current", 1)?);
        assert!(!host.unload_if_current("previous", 2)?);
        assert!(host.unload_if_current("current", 2)?);
        assert!(!host.unload_if_current("current", 2)?);
        Ok(())
    }
}
