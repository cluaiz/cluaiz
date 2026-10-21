//! Isolated GGUF embedding residency; existing planner/load parameters remain authoritative.
use anyhow::{bail, Context, Result};
use cluaiz_core::{contracts::EmbeddingOutput, ActiveUserSettings};
use cluaiz_models::{types::manifest::ModelRegistryEntry, GgufProber};
use llama_engine::native::embedding::EmbeddingModel;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// The same bounded whole-sequence settings are used by execution and plan preview.
pub fn capture_settings(
    geometry: &cluaiz_models::ModelGeometry,
    settings: &ActiveUserSettings,
) -> Result<ActiveUserSettings> {
    let mut captured = settings.clone();
    let requested = match captured.optimization.context_setting() {
        cluaiz_core::ContextSetting::Auto => captured.optimization.resolved_batch_size(),
        cluaiz_core::ContextSetting::Max => geometry.context_length,
        cluaiz_core::ContextSetting::Custom(tokens) => tokens,
    };
    let context = requested.min(geometry.context_length);
    if context == 0 {
        bail!("Embedding context limit is unavailable");
    }
    captured.optimization.custom_context_window = Some(context);
    captured.optimization.batch_size = Some(context);
    captured.optimization.ubatch_size = Some(context);
    Ok(captured)
}

struct Resident {
    id: String,
    key: String,
    model: EmbeddingModel,
    lease: Arc<()>,
}

#[derive(Clone, Default)]
pub struct EmbeddingHost {
    state: Arc<Mutex<Option<Resident>>>,
    transition: Arc<tokio::sync::Mutex<()>>,
    admission: crate::media_admission::MediaAdmission,
}

impl EmbeddingHost {
    pub(crate) fn with_transition(transition: Arc<tokio::sync::Mutex<()>>) -> Self {
        Self {
            transition,
            ..Self::default()
        }
    }

    /// Retain this permit through native work, including after an HTTP disconnect.
    pub fn try_acquire(
        &self,
    ) -> Result<crate::media_admission::MediaLease, tokio::sync::TryAcquireError> {
        self.admission.try_acquire()
    }

    /// Nonblocking status; a busy or empty owner has no currently reportable resident.
    pub fn resident_model(&self) -> Option<String> {
        self.state
            .try_lock()
            .ok()?
            .as_ref()
            .map(|resident| resident.id.clone())
    }

    /// Batches are serialized and evaluated on a blocking worker; chat state is untouched.
    pub async fn embed(
        &self,
        entry: ModelRegistryEntry,
        texts: Vec<String>,
        settings: ActiveUserSettings,
        keep_alive: Option<i32>,
        admission: crate::media_admission::MediaLease,
    ) -> Result<Vec<EmbeddingOutput>> {
        let owner = self.clone();
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut guard = cluaiz_media::request::PreparationGuard::new(Arc::clone(&cancellation));
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let lease = Arc::new(());
            let mut state = owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Embedding host lock poisoned"))?;
            let result = (|| {
                if cancellation.load(Ordering::Acquire) {
                    bail!("Embedding request cancelled");
                }
                let directory = std::fs::canonicalize(&entry.local_dir)?;
                let path = entry
                    .primary_path()
                    .context("Embedding primary file missing")?
                    .canonicalize()?;
                if !path.starts_with(&directory) || !path.is_file() {
                    bail!("Embedding file is outside its package");
                }
                let assets =
                    cluaiz_models::capabilities::PackageCapabilities::asset_snapshot(&directory);
                let key = serde_json::to_string(&(
                    &entry,
                    assets,
                    settings.optimization.load_fingerprint(),
                ))?;
                if state.as_ref().is_none_or(|resident| resident.key != key) {
                    let _transition = owner.transition.blocking_lock();
                    *state = None;
                    let geometry = GgufProber::probe_geometry(&path)?;
                    let hardware = cluaiz_hw::HardwareProber::with_live_memory();
                    let captured = capture_settings(&geometry, &settings)?;
                    let plan = cluaiz_plan::Planner::create_plan(
                        &entry.id, &geometry, &hardware, &captured,
                    );
                    if !plan.is_feasible() {
                        bail!("{}", plan.explanation);
                    }
                    let model = crate::host::native_load::load_embedding_with_plan(
                        path.to_str().context("UTF-8 model path")?,
                        &plan,
                        &captured,
                        &hardware,
                    )?;
                    *state = Some(Resident {
                        id: entry.id.clone(),
                        key,
                        model,
                        lease: Arc::clone(&lease),
                    });
                }
                let resident = state.as_mut().context("Embedding model unavailable")?;
                resident.lease = Arc::clone(&lease);
                let mut outputs = Vec::new();
                let mut bytes = 0u64;
                for text in texts {
                    if cancellation.load(Ordering::Acquire) {
                        bail!("Embedding request cancelled");
                    }
                    let (vector, tokens) = resident.model.embed(&text, settings.media.limits)?;
                    bytes = u64::try_from(vector.len())?
                        .checked_mul(4)
                        .and_then(|size| bytes.checked_add(size))
                        .context("Embedding output size overflow")?;
                    settings.media.limits.check_decoded_bytes(bytes)?;
                    outputs.push(EmbeddingOutput { vector, tokens });
                }
                if cancellation.load(Ordering::Acquire) {
                    bail!("Embedding request cancelled");
                }
                Ok(outputs)
            })();
            if result.is_err() || keep_alive == Some(0) {
                *state = None;
            }
            drop(state);
            if result.is_ok() {
                if let Some(minutes) = keep_alive.filter(|minutes| *minutes > 0) {
                    let weak = Arc::downgrade(&owner.state);
                    handle.spawn(async move {
                        tokio::time::sleep(Duration::from_secs(minutes as u64 * 60)).await;
                        if let Some(state) = weak.upgrade() {
                            let _ = tokio::task::spawn_blocking(move || {
                                if let Ok(mut state) = state.lock() {
                                    if state.as_ref().is_some_and(|resident| {
                                        Arc::ptr_eq(&resident.lease, &lease)
                                    }) {
                                        *state = None;
                                    }
                                }
                            })
                            .await;
                        }
                    });
                }
            }
            result
        })
        .await?;
        guard.complete();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_context_honors_auto_max_custom_and_missing_geometry() -> Result<()> {
        let geometry = cluaiz_models::ModelGeometry {
            context_length: 2048,
            ..Default::default()
        };
        let mut settings = ActiveUserSettings::default();
        let automatic = capture_settings(&geometry, &settings)?;
        assert_eq!(
            automatic.optimization.custom_context_window,
            Some(settings.optimization.resolved_batch_size().min(2048))
        );
        for selected in [0, u32::MAX, 4096, 128] {
            settings.optimization.custom_context_window = Some(selected);
            let captured = capture_settings(&geometry, &settings)?;
            let expected = if selected == 128 { 128 } else { 2048 };
            assert_eq!(captured.optimization.custom_context_window, Some(expected));
            assert_eq!(captured.optimization.batch_size, Some(expected));
            assert_eq!(captured.optimization.ubatch_size, Some(expected));
            assert_eq!(settings.optimization.custom_context_window, Some(selected));
        }
        assert!(capture_settings(&Default::default(), &settings).is_err());
        Ok(())
    }
}
